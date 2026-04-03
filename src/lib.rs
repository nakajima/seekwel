//! `seekwel` is a small SQLite library built around a global connection and
//! macro-generated models.
//!
//! Most applications will use:
//! - [`connection::Connection`] to initialize the global database connection.
//! - [`macro@model`] to declare database-backed structs.
//! - [`prelude`] to bring the query traits into scope.
//!
//! # Example
//!
//! ```rust
//! use seekwel::{Comparison, connection::Connection, prelude::*};
//!
//! #[seekwel::model]
//! struct Person {
//!     id: u64,
//!     name: String,
//!     age: Option<u8>,
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! Connection::memory()?;
//! Person::create_table()?;
//!
//! let pat = Person::builder().name("Pat").age(Some(20)).create()?;
//! let everyone = Person::all()?;
//! assert_eq!(everyone.len(), 1);
//!
//! let found = Person::q(PersonColumns::Name, Comparison::Eq("Pat")).first()?;
//! assert_eq!(found.map(|person| person.id), Some(pat.id));
//! # Ok(())
//! # }
//! ```

extern crate self as seekwel;

/// Global SQLite connection management.
pub mod connection;
/// Error types returned by seekwel operations.
pub mod error;
/// Model traits, query types, and SQLite field conversions.
pub mod model;
/// Low-level SQL string generation helpers.
pub mod sql;

pub use model::{
    Chunked, ChunkedIter, ChunkedQuery, ChunkedTryIter, Comparison, Lazy, LazyIter, LazyQuery,
    LazyTryIter, ModelQueryDsl, NewRecord, Persisted, Query, QueryDsl, SqlField,
};

/// Derive macro that implements seekwel's model traits for a typestate model struct.
pub use seekwel_macros::Model;
/// Attribute macro that turns a plain struct into a seekwel model.
pub use seekwel_macros::model;

/// Common trait imports for query building.
pub mod prelude {
    /// Model-level query entrypoints like `Person::all()` and `Person::lazy()`.
    pub use crate::ModelQueryDsl;
    /// Query-value chaining methods like `.q(...)`, `.and(...)`, and `.all()`.
    pub use crate::QueryDsl;
}
