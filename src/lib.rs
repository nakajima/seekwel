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
//! let mut pat = Person::builder().name("Pat").age(Some(20)).create()?;
//! let everyone = Person::all()?;
//! assert_eq!(everyone.len(), 1);
//!
//! pat.age = None;
//! pat.save()?;
//!
//! let found = Person::q(PersonColumns::Name, Comparison::Eq("Pat")).first()?;
//! assert_eq!(found.map(|person| person.id), Some(pat.id));
//!
//! let pat_id = pat.id;
//! pat.delete()?;
//! assert!(Person::find(pat_id).is_err());
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
/// Schema planning and migration helpers.
pub mod schema;
mod sql;

#[doc(hidden)]
pub mod __private {
    #[cfg(feature = "serde")]
    pub use serde;
}

pub use error::Error;
/// Re-export of the SQLite driver used by seekwel.
pub use rusqlite;

pub use model::{
    BelongsTo, Chunked, ChunkedIter, ChunkedQuery, ChunkedTryIter, Comparison, CreateOrUpdateError,
    Errors, HasMany, HasManyQuery, IndexDef, Invalid, InvalidModel, Lazy, LazyIter, LazyQuery,
    LazyTryIter, Model, ModelQueryDsl, ModelRecord, NewModel, NewRecord, NoValidation, Order,
    Params, ParamsModel, ParamsModelDsl, Persisted, PersistedModel, PrimaryKeyField,
    PrimaryKeyLookup, Query, QueryDsl, SaveError, SqlField, ValidationError, Validator,
};

/// Derive macro that implements seekwel's model traits for a typestate model struct.
pub use seekwel_macros::Model;
/// Attribute macro that turns a plain struct into a seekwel model.
pub use seekwel_macros::model;

/// Common trait imports for query building.
pub mod prelude {
    /// Accessors for invalid model values returned by validation failures.
    pub use crate::InvalidModel;
    /// Model-level query entrypoints like `Person::all()` and `Person::lazy()`.
    pub use crate::ModelQueryDsl;
    /// Record helpers like `.errors()`, `.is_persisted()`, and `.is_new_record()`.
    pub use crate::ModelRecord;
    /// New-record operations like `.save()`.
    pub use crate::NewModel;
    /// Params filtering helpers like `.allow(...)`.
    pub use crate::Params;
    /// Model-level params entrypoints like `Person::new(...)` and `person.update(...)`.
    pub use crate::ParamsModelDsl;
    /// Persisted-record operations like `find(...)`, `.save()`, and `.delete()`.
    pub use crate::PersistedModel;
    /// Query-value chaining methods like `.q(...)`, `.and(...)`, and `.all()`.
    pub use crate::QueryDsl;
    /// Typed column helpers like `PersonColumns::Name.asc()` and `.desc()`.
    pub use crate::model::Column;
}
