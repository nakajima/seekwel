//! Model traits, query types, and SQLite field conversions.
//!
//! Most users interact with this module indirectly through the crate root and
//! the `#[seekwel::model]` macro.

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;
use crate::sql;

/// Builder support types used by generated model builders.
pub mod builder;
mod comparison;
mod query;
mod sql_field;

pub use comparison::{Comparison, ComparisonOperand};
pub use query::{
    Chunked, ChunkedIter, ChunkedQuery, ChunkedTryIter, Lazy, LazyIter, LazyQuery, LazyTryIter,
    ModelQueryDsl, Query, QueryDsl,
};
pub use sql_field::SqlField;
#[doc(hidden)]
pub use sql_field::column;

/// Describes a non-`id` database column for a model.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    /// The column name in SQLite.
    pub name: &'static str,
    /// The SQLite type name used in `CREATE TABLE` statements.
    pub sql_type: &'static str,
    /// Whether the column may store `NULL`.
    pub nullable: bool,
}

/// A typed column identifier used by generated query APIs.
///
/// This trait is implemented for the generated `<ModelName>Columns` enum that
/// the model macro creates next to each model type.
pub trait Column: Copy + Clone {
    /// Returns the SQL column name for this typed column.
    fn as_str(self) -> &'static str;
}

/// Typestate marker for an in-memory record that has not been saved yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NewRecord;

/// Typestate marker for a record that already exists in the database.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Persisted;

/// Core behavior shared by all seekwel model types.
pub trait Model: Sized {
    /// The typed column enum generated for this model.
    type Column: Column;

    /// Returns the SQLite table name for this model.
    fn table_name() -> &'static str;
    /// Returns metadata for all non-`id` columns in declaration order.
    fn columns() -> &'static [ColumnDef];
    /// Returns values for non-`id` columns, in the same order as [`Self::columns`].
    fn params(&self) -> Vec<Value>;

    /// Creates the model's SQLite table if it does not already exist.
    fn create_table() -> Result<(), Error> {
        let conn = Connection::get()?;
        conn.execute(&sql::create_table(Self::table_name(), Self::columns()), ())?;
        Ok(())
    }
}

/// Behavior available only for persisted model values.
pub trait PersistedModel: Model + Sized {
    /// Returns the model's primary key value.
    fn id(&self) -> u64;
    /// Builds a persisted model instance from a SQLite row.
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>;

    /// Loads a persisted model by primary key.
    fn find(id: u64) -> Result<Self, Error> {
        let conn = Connection::get()?;
        conn.query_row(
            &sql::select_by_id(Self::table_name(), Self::columns()),
            [id as i64],
            Self::from_row,
        )
    }

    /// Persists the current in-memory field values back to the database.
    fn save(&self) -> Result<(), Error> {
        let mut params = self.params();
        params.push(Value::Integer(self.id() as i64));

        let conn = Connection::get()?;
        let changed = conn.execute(
            &sql::update_by_id(Self::table_name(), Self::columns()),
            params_from_iter(params),
        )?;

        if changed == 0 {
            return Err(Error::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }

        Ok(())
    }

    /// Re-fetches the current row from the database and overwrites `self`.
    fn reload(&mut self) -> Result<(), Error> {
        *self = Self::find(self.id())?;
        Ok(())
    }

    /// Deletes this persisted row from the database.
    fn delete(self) -> Result<(), Error> {
        let conn = Connection::get()?;
        let changed = conn.execute(&sql::delete_by_id(Self::table_name()), [self.id() as i64])?;

        if changed == 0 {
            return Err(Error::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }

        Ok(())
    }
}

/// Inserts a new model record and returns its generated primary key.
///
/// This is mainly used by code generated from the model macro.
pub fn insert<M: Model>(model: &M) -> Result<u64, Error> {
    let params = model.params();
    let conn = Connection::get()?;
    conn.insert(
        &sql::insert(M::table_name(), M::columns()),
        params_from_iter(params),
    )
}
