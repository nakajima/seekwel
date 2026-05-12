//! Global SQLite connection management.
//!
//! `seekwel` intentionally exposes process-wide connection setup, but file
//! databases use an internal connection pool. Initialize it once with
//! [`Connection::memory`](crate::connection::Connection::memory) or
//! [`Connection::file`](crate::connection::Connection::file), then retrieve a
//! lightweight handle with [`Connection::get`](crate::connection::Connection::get).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::ptr::NonNull;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Params};

use crate::error::Error;

const DEFAULT_FILE_READERS: usize = 4;

static GLOBAL: OnceLock<ConnectionManager> = OnceLock::new();
static QUERY_LOG: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

thread_local! {
    static TX_STATE: RefCell<Option<TransactionState>> = const { RefCell::new(None) };
}

struct TransactionState {
    conn: NonNull<rusqlite::Connection>,
    depth: usize,
    next_savepoint: usize,
}

/// A lightweight handle to the process-wide SQLite connection manager.
///
/// Cloning or copying this handle does not reserve a database connection.
/// Each method checks out an appropriate connection for the duration of that
/// method call, unless the current thread is inside [`Connection::transaction`].
#[derive(Debug, Clone, Copy)]
pub struct Connection {
    _private: (),
}

struct ConnectionManager {
    writer: Mutex<rusqlite::Connection>,
    readers: Option<ReaderPool>,
    gate: RwLock<()>,
}

struct ReaderPool {
    conns: Mutex<Vec<rusqlite::Connection>>,
    available: Condvar,
}

struct ReaderCheckout<'a> {
    conn: Option<rusqlite::Connection>,
    pool: &'a ReaderPool,
}

impl Connection {
    /// Initializes the global connection from a SQLite database file.
    ///
    /// File databases use WAL mode with one writer connection and a small pool
    /// of read-only reader connections. Returns [`Error::AlreadyInitialized`]
    /// if a global connection has already been set.
    pub fn file(path: &str) -> Result<(), Error> {
        let manager = run_blocking(|| ConnectionManager::file(path, DEFAULT_FILE_READERS))?;
        clear_query_log();
        GLOBAL.set(manager).map_err(|_| Error::AlreadyInitialized)
    }

    /// Initializes the global connection with an in-memory SQLite database.
    ///
    /// In-memory databases are connection-local in SQLite, so this mode always
    /// uses a single connection. Returns [`Error::AlreadyInitialized`] if a
    /// global connection has already been set.
    ///
    /// ```rust
    /// use seekwel::connection::Connection;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// Connection::memory()?;
    /// let conn = Connection::get()?;
    /// conn.execute("CREATE TABLE things (id INTEGER PRIMARY KEY)", ())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn memory() -> Result<(), Error> {
        let manager = run_blocking(ConnectionManager::memory)?;
        clear_query_log();
        GLOBAL.set(manager).map_err(|_| Error::AlreadyInitialized)
    }

    /// Returns a lightweight handle to the global connection manager.
    ///
    /// Returns [`Error::NotInitialized`] if no global connection has been
    /// created yet.
    pub fn get() -> Result<Self, Error> {
        manager()?;
        Ok(Self { _private: () })
    }

    /// Runs `f` inside a SQLite transaction on the writer connection.
    ///
    /// While the closure runs, all seekwel operations on the current thread use
    /// the same transaction connection implicitly. Other writes wait for the
    /// transaction to finish. For file databases in WAL mode, reads on other
    /// threads may continue and see the last committed snapshot.
    ///
    /// Nested calls use SQLite savepoints.
    pub fn transaction<T, F>(f: F) -> Result<T, Error>
    where
        F: FnOnce() -> Result<T, Error>,
    {
        if transaction_active() {
            nested_transaction(f)
        } else {
            run_blocking(|| outer_transaction(f))
        }
    }

    /// Convenience helper for intentionally rolling back a transaction closure.
    pub fn rollback<T>() -> Result<T, Error> {
        Err(Error::Rollback)
    }

    /// Executes a statement and returns the number of changed rows.
    pub fn execute<P: Params>(&self, query: &str, params: P) -> Result<usize, Error> {
        Self::with_write(|conn| conn.execute(query, params).map_err(Error::Sqlite))
    }

    /// Executes an insert statement and returns the last inserted row id.
    pub fn insert<P: Params>(&self, query: &str, params: P) -> Result<u64, Error> {
        Self::with_write(|conn| {
            conn.execute(query, params).map_err(Error::Sqlite)?;
            Ok(conn.last_insert_rowid() as u64)
        })
    }

    /// Executes a query expected to return exactly one row.
    pub fn query_row<T, P, F>(&self, query: &str, params: P, f: F) -> Result<T, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        Self::with_write(|conn| conn.query_row(query, params, f).map_err(Error::Sqlite))
    }

    /// Executes a query and returns either zero or one row.
    pub fn query_optional<T, P, F>(&self, query: &str, params: P, f: F) -> Result<Option<T>, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        Self::with_write(|conn| {
            conn.query_row(query, params, f)
                .optional()
                .map_err(Error::Sqlite)
        })
    }

    /// Executes a query and collects all returned rows into a vector.
    pub fn query_all<T, P, F>(&self, query: &str, params: P, f: F) -> Result<Vec<T>, Error>
    where
        P: Params,
        F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        Self::with_write(|conn| query_all_on(conn, query, params, f))
    }

    pub(crate) fn query_row_read<T, P, F>(&self, query: &str, params: P, f: F) -> Result<T, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        Self::with_read(|conn| conn.query_row(query, params, f).map_err(Error::Sqlite))
    }

    pub(crate) fn query_optional_read<T, P, F>(
        &self,
        query: &str,
        params: P,
        f: F,
    ) -> Result<Option<T>, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        Self::with_read(|conn| {
            conn.query_row(query, params, f)
                .optional()
                .map_err(Error::Sqlite)
        })
    }

    pub(crate) fn query_all_read<T, P, F>(
        &self,
        query: &str,
        params: P,
        f: F,
    ) -> Result<Vec<T>, Error>
    where
        P: Params,
        F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        Self::with_read(|conn| query_all_on(conn, query, params, f))
    }

    pub(crate) fn with_read<T, F>(f: F) -> Result<T, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
    {
        if transaction_active() {
            return with_required_transaction_connection(f);
        }

        run_blocking(|| manager()?.with_read(f))
    }

    pub(crate) fn with_write<T, F>(f: F) -> Result<T, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
    {
        if transaction_active() {
            return with_required_transaction_connection(f);
        }

        run_blocking(|| manager()?.with_write(f))
    }

    pub(crate) fn with_exclusive_write<T, F>(f: F) -> Result<T, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
    {
        if transaction_active() {
            return Err(Error::InvalidSchema(
                "exclusive schema operations cannot run inside a transaction".to_string(),
            ));
        }

        run_blocking(|| manager()?.with_exclusive_write(f))
    }

    /// Returns the most recent SQL strings seekwel attempted to execute.
    pub fn recent_queries() -> Vec<String> {
        lock(query_log()).iter().cloned().collect()
    }
}

fn query_all_on<T, P, F>(
    conn: &rusqlite::Connection,
    query: &str,
    params: P,
    f: F,
) -> Result<Vec<T>, Error>
where
    P: Params,
    F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(query).map_err(Error::Sqlite)?;
    let rows = stmt.query_map(params, f).map_err(Error::Sqlite)?;
    let mut values = Vec::new();

    for row in rows {
        values.push(row.map_err(Error::Sqlite)?);
    }

    Ok(values)
}

fn run_blocking<T, F>(f: F) -> T
where
    F: FnOnce() -> T,
{
    #[cfg(feature = "tokio")]
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current()
            && handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread
        {
            return tokio::task::block_in_place(f);
        }
    }

    f()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl ConnectionManager {
    fn memory() -> Result<Self, Error> {
        let writer = rusqlite::Connection::open_in_memory()?;
        Ok(Self {
            writer: Mutex::new(writer),
            readers: None,
            gate: RwLock::new(()),
        })
    }

    fn file(path: &str, reader_count: usize) -> Result<Self, Error> {
        let writer = rusqlite::Connection::open(path)?;
        configure_file_writer(&writer)?;

        let readers = ReaderPool::file(path, reader_count)?;
        Ok(Self {
            writer: Mutex::new(writer),
            readers: Some(readers),
            gate: RwLock::new(()),
        })
    }

    fn with_read<T, F>(&self, f: F) -> Result<T, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
    {
        if let Some(readers) = &self.readers {
            let checkout = readers.checkout();
            let _gate = read_lock(&self.gate);
            f(checkout.conn())
        } else {
            let _gate = read_lock(&self.gate);
            let writer = lock(&self.writer);
            f(&writer)
        }
    }

    fn with_write<T, F>(&self, f: F) -> Result<T, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
    {
        let _gate = read_lock(&self.gate);
        let writer = lock(&self.writer);
        f(&writer)
    }

    fn with_exclusive_write<T, F>(&self, f: F) -> Result<T, Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
    {
        let _gate = write_lock(&self.gate);
        let writer = lock(&self.writer);
        f(&writer)
    }
}

impl ReaderPool {
    fn file(path: &str, count: usize) -> Result<Self, Error> {
        let mut conns = Vec::with_capacity(count);
        for _ in 0..count {
            let conn = rusqlite::Connection::open(path)?;
            configure_file_reader(&conn)?;
            conns.push(conn);
        }

        Ok(Self {
            conns: Mutex::new(conns),
            available: Condvar::new(),
        })
    }

    fn checkout(&self) -> ReaderCheckout<'_> {
        let mut conns = lock(&self.conns);
        loop {
            if let Some(conn) = conns.pop() {
                return ReaderCheckout {
                    conn: Some(conn),
                    pool: self,
                };
            }

            conns = self
                .available
                .wait(conns)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

impl ReaderCheckout<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.conn
            .as_ref()
            .expect("reader checkout should always hold a connection")
    }
}

impl Drop for ReaderCheckout<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let mut conns = lock(&self.pool.conns);
            conns.push(conn);
            self.pool.available.notify_one();
        }
    }
}

fn configure_file_writer(conn: &rusqlite::Connection) -> Result<(), Error> {
    conn.execute_batch("PRAGMA journal_mode = WAL")
        .map_err(Error::Sqlite)
}

fn configure_file_reader(conn: &rusqlite::Connection) -> Result<(), Error> {
    conn.execute_batch("PRAGMA query_only = ON")
        .map_err(Error::Sqlite)
}

fn outer_transaction<T, F>(f: F) -> Result<T, Error>
where
    F: FnOnce() -> Result<T, Error>,
{
    let manager = manager()?;
    let _gate = read_lock(&manager.gate);
    let writer = lock(&manager.writer);

    begin_immediate(&writer)?;
    set_transaction_connection(&writer);

    let result = panic::catch_unwind(AssertUnwindSafe(f));
    clear_transaction_connection();

    match result {
        Ok(Ok(value)) => {
            if let Err(error) = commit(&writer) {
                let _ = rollback(&writer);
                return Err(error);
            }
            Ok(value)
        }
        Ok(Err(error)) => {
            rollback(&writer)?;
            Err(error)
        }
        Err(payload) => {
            let _ = rollback(&writer);
            panic::resume_unwind(payload);
        }
    }
}

fn nested_transaction<T, F>(f: F) -> Result<T, Error>
where
    F: FnOnce() -> Result<T, Error>,
{
    let savepoint = begin_savepoint()?;
    let result = panic::catch_unwind(AssertUnwindSafe(f));

    match result {
        Ok(Ok(value)) => {
            let release = release_savepoint(&savepoint);
            finish_savepoint();
            release?;
            Ok(value)
        }
        Ok(Err(error)) => {
            let _ = rollback_to_savepoint(&savepoint);
            let _ = release_savepoint(&savepoint);
            finish_savepoint();
            Err(error)
        }
        Err(payload) => {
            let _ = rollback_to_savepoint(&savepoint);
            let _ = release_savepoint(&savepoint);
            finish_savepoint();
            panic::resume_unwind(payload);
        }
    }
}

fn begin_savepoint() -> Result<String, Error> {
    let savepoint = TX_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state
            .as_mut()
            .expect("nested transaction should have transaction state");
        state.depth += 1;
        state.next_savepoint += 1;
        format!("__seekwel_savepoint_{}", state.next_savepoint)
    });

    let sql = format!("SAVEPOINT {savepoint}");
    record_query(&sql);
    match with_required_transaction_connection(|conn| {
        conn.execute_batch(&sql).map_err(Error::Sqlite)
    }) {
        Ok(()) => Ok(savepoint),
        Err(error) => {
            finish_savepoint();
            Err(error)
        }
    }
}

fn release_savepoint(savepoint: &str) -> Result<(), Error> {
    let sql = format!("RELEASE SAVEPOINT {savepoint}");
    record_query(&sql);
    with_required_transaction_connection(|conn| conn.execute_batch(&sql).map_err(Error::Sqlite))
}

fn rollback_to_savepoint(savepoint: &str) -> Result<(), Error> {
    let sql = format!("ROLLBACK TO SAVEPOINT {savepoint}");
    record_query(&sql);
    with_required_transaction_connection(|conn| conn.execute_batch(&sql).map_err(Error::Sqlite))
}

fn finish_savepoint() {
    TX_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state
            .as_mut()
            .expect("savepoint should have transaction state");
        state.depth -= 1;
    });
}

fn begin_immediate(conn: &rusqlite::Connection) -> Result<(), Error> {
    record_query("BEGIN IMMEDIATE");
    conn.execute_batch("BEGIN IMMEDIATE").map_err(Error::Sqlite)
}

fn commit(conn: &rusqlite::Connection) -> Result<(), Error> {
    record_query("COMMIT");
    conn.execute_batch("COMMIT").map_err(Error::Sqlite)
}

fn rollback(conn: &rusqlite::Connection) -> Result<(), Error> {
    record_query("ROLLBACK");
    conn.execute_batch("ROLLBACK").map_err(Error::Sqlite)
}

fn transaction_active() -> bool {
    TX_STATE.with(|state| state.borrow().is_some())
}

fn set_transaction_connection(conn: &rusqlite::Connection) {
    TX_STATE.with(|state| {
        *state.borrow_mut() = Some(TransactionState {
            conn: NonNull::from(conn),
            depth: 1,
            next_savepoint: 0,
        });
    });
}

fn clear_transaction_connection() {
    TX_STATE.with(|state| {
        *state.borrow_mut() = None;
    });
}

fn with_transaction_connection<T, F>(f: F) -> Option<Result<T, Error>>
where
    F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
{
    let conn = TX_STATE.with(|state| state.borrow().as_ref().map(|state| state.conn));
    conn.map(|conn| {
        let conn = unsafe { conn.as_ref() };
        f(conn)
    })
}

fn with_required_transaction_connection<T, F>(f: F) -> Result<T, Error>
where
    F: FnOnce(&rusqlite::Connection) -> Result<T, Error>,
{
    with_transaction_connection(f).expect("transaction connection should be active")
}

fn manager() -> Result<&'static ConnectionManager, Error> {
    GLOBAL.get().ok_or(Error::NotInitialized)
}

pub(crate) fn record_query(query: &str) {
    push_query_log(normalize_query(query));
}

pub(crate) fn record_query_with_params(query: &str, params: &[Value]) {
    let normalized = normalize_query(query);
    if normalized.is_empty() {
        return;
    }

    if params.is_empty() {
        push_query_log(normalized);
        return;
    }

    let rendered_params = params
        .iter()
        .enumerate()
        .map(|(index, value)| format!("?{}={}", index + 1, render_value(value)))
        .collect::<Vec<_>>()
        .join(", ");
    push_query_log(format!("{normalized} -- [{rendered_params}]"));
}

fn push_query_log(entry: String) {
    if entry.is_empty() {
        return;
    }

    log::debug!(target: "seekwel::sql", "  SQL  {entry}");

    let mut log = lock(query_log());
    if log.len() >= 100 {
        log.pop_front();
    }
    log.push_back(entry);
}

fn query_log() -> &'static Mutex<VecDeque<String>> {
    QUERY_LOG.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn clear_query_log() {
    lock(query_log()).clear();
}

fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Real(value) => value.to_string(),
        Value::Text(value) => format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''")),
        Value::Blob(bytes) => {
            let hex = bytes
                .iter()
                .map(|byte| format!("{:02X}", byte))
                .collect::<String>();
            format!("x'{hex}'")
        }
    }
}
