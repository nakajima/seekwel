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
    /// The requested model configuration or conversion is invalid.
    InvalidModel(String),
    /// The requested query cannot be represented safely.
    InvalidQuery(String),
    /// The requested association operation is invalid for the current model state.
    InvalidAssociation(String),
    /// The requested schema operation cannot be represented safely.
    InvalidSchema(String),
    /// A schema plan exists, but cannot be applied automatically.
    SchemaBlocked(String),
    /// The live schema drifted from the source schema used to build a plan.
    SchemaDrift(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sqlite(e) => write!(f, "{e}"),
            Error::AlreadyInitialized => write!(f, "Connection already initialized"),
            Error::NotInitialized => write!(f, "Connection not initialized"),
            Error::MissingField(field) => write!(f, "Missing required field: {field}"),
            Error::InvalidModel(message) => write!(f, "Invalid model: {message}"),
            Error::InvalidQuery(message) => write!(f, "Invalid query: {message}"),
            Error::InvalidAssociation(message) => write!(f, "Invalid association: {message}"),
            Error::InvalidSchema(message) => write!(f, "Invalid schema: {message}"),
            Error::SchemaBlocked(message) => write!(f, "Schema plan blocked: {message}"),
            Error::SchemaDrift(message) => write!(f, "Schema drift detected: {message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}
