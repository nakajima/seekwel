//! Global SQLite connection management.
//!
//! `seekwel` intentionally uses a single global connection for the whole
//! process. Initialize it once with
//! [`Connection::memory`](crate::connection::Connection::memory) or
//! [`Connection::file`](crate::connection::Connection::file), then retrieve it
//! with [`Connection::get`](crate::connection::Connection::get).

use std::sync::{Mutex, MutexGuard, OnceLock};

use rusqlite::{OptionalExtension, Params};

use crate::error::Error;

static GLOBAL: OnceLock<Mutex<Connection>> = OnceLock::new();

/// The process-wide SQLite connection wrapper used by seekwel.
pub struct Connection {
    conn: rusqlite::Connection,
}

impl Connection {
    /// Initializes the global connection from a SQLite database file.
    ///
    /// Returns [`Error::AlreadyInitialized`] if a global connection has already
    /// been set.
    pub fn file(path: &str) -> Result<(), Error> {
        let conn = rusqlite::Connection::open(path)?;
        GLOBAL
            .set(Mutex::new(Connection { conn }))
            .map_err(|_| Error::AlreadyInitialized)
    }

    /// Initializes the global connection with an in-memory SQLite database.
    ///
    /// Returns [`Error::AlreadyInitialized`] if a global connection has already
    /// been set.
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
        let conn = rusqlite::Connection::open_in_memory()?;
        GLOBAL
            .set(Mutex::new(Connection { conn }))
            .map_err(|_| Error::AlreadyInitialized)
    }

    /// Returns a guard to the global connection.
    ///
    /// Returns [`Error::NotInitialized`] if no global connection has been
    /// created yet.
    pub fn get() -> Result<MutexGuard<'static, Connection>, Error> {
        GLOBAL
            .get()
            .ok_or(Error::NotInitialized)
            .map(|m| m.lock().unwrap())
    }

    /// Executes a statement and returns the number of changed rows.
    pub fn execute<P: Params>(&self, query: &str, params: P) -> Result<usize, Error> {
        self.conn.execute(query, params).map_err(Error::Sqlite)
    }

    /// Executes an insert statement and returns the last inserted row id.
    pub fn insert<P: Params>(&self, query: &str, params: P) -> Result<u64, Error> {
        self.conn.execute(query, params).map_err(Error::Sqlite)?;
        Ok(self.conn.last_insert_rowid() as u64)
    }

    /// Executes a query expected to return exactly one row.
    pub fn query_row<T, P, F>(&self, query: &str, params: P, f: F) -> Result<T, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        self.conn.query_row(query, params, f).map_err(Error::Sqlite)
    }

    /// Executes a query and returns either zero or one row.
    pub fn query_optional<T, P, F>(
        &self,
        query: &str,
        params: P,
        f: F,
    ) -> Result<Option<T>, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        self.conn
            .query_row(query, params, f)
            .optional()
            .map_err(Error::Sqlite)
    }

    /// Executes a query and collects all returned rows into a vector.
    pub fn query_all<T, P, F>(&self, query: &str, params: P, f: F) -> Result<Vec<T>, Error>
    where
        P: Params,
        F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        let mut stmt = self.conn.prepare(query).map_err(Error::Sqlite)?;
        let rows = stmt.query_map(params, f).map_err(Error::Sqlite)?;
        let mut values = Vec::new();

        for row in rows {
            values.push(row.map_err(Error::Sqlite)?);
        }

        Ok(values)
    }
}
