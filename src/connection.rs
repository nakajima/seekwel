use std::sync::{Mutex, MutexGuard, OnceLock};

use rusqlite::{OptionalExtension, Params};

use crate::error::Error;

static GLOBAL: OnceLock<Mutex<Connection>> = OnceLock::new();

pub struct Connection {
    conn: rusqlite::Connection,
}

impl Connection {
    pub fn file(path: &str) -> Result<(), Error> {
        let conn = rusqlite::Connection::open(path)?;
        GLOBAL
            .set(Mutex::new(Connection { conn }))
            .map_err(|_| Error::AlreadyInitialized)
    }

    pub fn memory() -> Result<(), Error> {
        let conn = rusqlite::Connection::open_in_memory()?;
        GLOBAL
            .set(Mutex::new(Connection { conn }))
            .map_err(|_| Error::AlreadyInitialized)
    }

    pub fn get() -> Result<MutexGuard<'static, Connection>, Error> {
        GLOBAL
            .get()
            .ok_or(Error::NotInitialized)
            .map(|m| m.lock().unwrap())
    }

    pub fn execute<P: Params>(&self, query: &str, params: P) -> Result<usize, Error> {
        self.conn.execute(query, params).map_err(Error::Sqlite)
    }

    pub fn insert<P: Params>(&self, query: &str, params: P) -> Result<u64, Error> {
        self.conn.execute(query, params).map_err(Error::Sqlite)?;
        Ok(self.conn.last_insert_rowid() as u64)
    }

    pub fn query_row<T, P, F>(&self, query: &str, params: P, f: F) -> Result<T, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        self.conn.query_row(query, params, f).map_err(Error::Sqlite)
    }

    pub fn query_optional<T, P, F>(&self, query: &str, params: P, f: F) -> Result<Option<T>, Error>
    where
        P: Params,
        F: FnOnce(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        self.conn
            .query_row(query, params, f)
            .optional()
            .map_err(Error::Sqlite)
    }

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
