use std::fmt;

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    AlreadyInitialized,
    NotInitialized,
    MissingField(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "{e}"),
            Error::AlreadyInitialized => write!(f, "Connection already initialized"),
            Error::NotInitialized => write!(f, "Connection not initialized"),
            Error::MissingField(field) => write!(f, "Missing required field: {field}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}
