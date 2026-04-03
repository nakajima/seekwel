//! Error types returned by seekwel.

use std::fmt;

/// Errors produced by connection, model, and query operations.
#[derive(Debug)]
pub enum Error {
    /// An error returned by `rusqlite`.
    Sqlite(rusqlite::Error),
    /// The global connection was initialized more than once.
    AlreadyInitialized,
    /// The global connection was used before initialization.
    NotInitialized,
    /// A required builder field was not provided.
    MissingField(String),
    /// The requested query cannot be represented safely.
    InvalidQuery(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "{e}"),
            Error::AlreadyInitialized => write!(f, "Connection already initialized"),
            Error::NotInitialized => write!(f, "Connection not initialized"),
            Error::MissingField(field) => write!(f, "Missing required field: {field}"),
            Error::InvalidQuery(message) => write!(f, "Invalid query: {message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}
