//! Model traits, query types, and SQLite field conversions.
//!
//! Most users interact with this module indirectly through the crate root and
//! the `#[seekwel::model]` macro.

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::{Connection, record_query, record_query_with_params};
use crate::error::Error;
use crate::sql;

mod association;
/// Builder support types used by generated model builders.
pub mod builder;
mod comparison;
mod query;
mod sql_field;

#[doc(hidden)]
pub use association::HasManyAssociation;
pub use association::{BelongsTo, HasMany};
pub use comparison::{Comparison, ComparisonOperand};
pub use query::{
    Chunked, ChunkedIter, ChunkedQuery, ChunkedTryIter, Lazy, LazyIter, LazyQuery, LazyTryIter,
    ModelQueryDsl, Order, Query, QueryDsl,
};
pub use sql_field::SqlField;
#[doc(hidden)]
pub use sql_field::column;

/// Describes a non-primary-key database column for a model.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    /// The column name in SQLite.
    pub name: &'static str,
    /// The SQLite type name used in `CREATE TABLE` statements.
    pub sql_type: &'static str,
    /// Whether the column may store `NULL`.
    pub nullable: bool,
}

/// Describes a model's primary key column.
#[derive(Debug, Clone, Copy)]
pub struct PrimaryKeyDef {
    /// The primary-key column name in SQLite.
    pub name: &'static str,
    /// The SQLite type name used in `CREATE TABLE` statements.
    pub sql_type: &'static str,
    /// Whether inserts omit the primary-key column and rely on SQLite to generate it.
    pub auto_increment: bool,
}

/// A primary-key field type supported by seekwel's model code generation.
pub trait PrimaryKeyField: SqlField {
    /// Converts the field into the non-negative association id used by `BelongsTo` and `HasMany`.
    fn to_association_id(&self) -> Result<u64, Error>;

    /// Converts a generated SQLite rowid into the Rust primary-key field type.
    fn from_generated_id(id: u64) -> Result<Self, Error>;
}

/// A value accepted by generated `find(...)` helpers for integer primary keys.
pub trait PrimaryKeyLookup {
    /// Converts the lookup value into a bound SQLite parameter.
    fn into_primary_key_value(self) -> Value;
}

macro_rules! impl_primary_key_lookup {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PrimaryKeyLookup for $ty {
                fn into_primary_key_value(self) -> Value {
                    <Self as SqlField>::to_sql_value(&self)
                }
            }
        )*
    };
}

impl_primary_key_lookup!(u64, u32, u16, u8, i64, i32, i16, i8, Value);

macro_rules! impl_unsigned_primary_key_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PrimaryKeyField for $ty {
                fn to_association_id(&self) -> Result<u64, Error> {
                    Ok(*self as u64)
                }

                fn from_generated_id(id: u64) -> Result<Self, Error> {
                    <$ty>::try_from(id).map_err(|_| {
                        Error::InvalidModel(format!(
                            "generated primary key {id} does not fit in {}",
                            stringify!($ty)
                        ))
                    })
                }
            }
        )*
    };
}

macro_rules! impl_signed_primary_key_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PrimaryKeyField for $ty {
                fn to_association_id(&self) -> Result<u64, Error> {
                    u64::try_from(*self).map_err(|_| {
                        Error::InvalidModel(format!(
                            "primary key value {} cannot be represented as a non-negative association id",
                            *self
                        ))
                    })
                }

                fn from_generated_id(id: u64) -> Result<Self, Error> {
                    <$ty>::try_from(id).map_err(|_| {
                        Error::InvalidModel(format!(
                            "generated primary key {id} does not fit in {}",
                            stringify!($ty)
                        ))
                    })
                }
            }
        )*
    };
}

impl_unsigned_primary_key_field!(u64, u32, u16, u8);
impl_signed_primary_key_field!(i64, i32, i16, i8);

/// A typed column identifier used by generated query APIs.
///
/// This trait is implemented for the generated `<ModelName>Columns` enum that
/// the model macro creates next to each model type.
pub trait Column: Copy + Clone {
    /// Returns the SQL column name for this typed column.
    fn as_str(self) -> &'static str;

    /// Returns an ascending `ORDER BY` clause for this column.
    fn asc(self) -> Order
    where
        Self: Sized,
    {
        Order::asc(self)
    }

    /// Returns a descending `ORDER BY` clause for this column.
    fn desc(self) -> Order
    where
        Self: Sized,
    {
        Order::desc(self)
    }
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
    /// Returns the model's primary-key column metadata.
    fn primary_key() -> PrimaryKeyDef;
    /// Returns metadata for all non-primary-key stored columns in declaration order.
    fn columns() -> &'static [ColumnDef];
    /// Returns metadata for the columns used by `INSERT` statements.
    fn insert_columns() -> &'static [ColumnDef];
    /// Returns values for non-primary-key stored columns, in the same order as [`Self::columns`].
    fn params(&self) -> Vec<Value>;
    /// Returns values for `INSERT` statements, in the same order as [`Self::insert_columns`].
    fn insert_params(&self) -> Vec<Value>;

    /// Creates the model's SQLite table if it does not already exist.
    fn create_table() -> Result<(), Error> {
        let conn = Connection::get()?;
        let query = sql::create_table(Self::table_name(), Self::primary_key(), Self::columns());
        record_query(&query);
        conn.execute(&query, ())?;
        Ok(())
    }
}

/// Behavior available only for persisted model values.
pub trait PersistedModel: Model + Sized {
    /// Returns the model's primary key in the non-negative association-id representation.
    fn id(&self) -> u64;
    /// Returns the model's primary-key value as a SQLite parameter.
    fn primary_key_value(&self) -> Value;
    /// Builds a persisted model instance from a SQLite row.
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>;

    /// Loads a persisted model by primary key.
    fn find<K>(id: K) -> Result<Self, Error>
    where
        K: PrimaryKeyLookup,
    {
        let conn = Connection::get()?;
        let query = sql::select_by_primary_key(Self::table_name(), Self::primary_key(), Self::columns());
        let param = id.into_primary_key_value();
        let params = vec![param.clone()];
        record_query_with_params(&query, &params);
        conn.query_row(&query, params_from_iter(params), Self::from_row)
    }

    /// Persists the current in-memory field values back to the database.
    fn save(&self) -> Result<(), Error> {
        let mut params = self.params();
        params.push(self.primary_key_value());

        let conn = Connection::get()?;
        let query = sql::update_by_primary_key(Self::table_name(), Self::primary_key(), Self::columns());
        record_query_with_params(&query, &params);
        let changed = conn.execute(&query, params_from_iter(params))?;

        if changed == 0 {
            return Err(Error::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }

        Ok(())
    }

    /// Re-fetches the current row from the database and overwrites `self`.
    fn reload(&mut self) -> Result<(), Error> {
        *self = Self::find(self.primary_key_value())?;
        Ok(())
    }

    /// Deletes this persisted row from the database.
    fn delete(self) -> Result<(), Error> {
        let conn = Connection::get()?;
        let query = sql::delete_by_primary_key(Self::table_name(), Self::primary_key());
        let params = vec![self.primary_key_value()];
        record_query_with_params(&query, &params);
        let changed = conn.execute(&query, params_from_iter(params))?;

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
    let params = model.insert_params();
    let conn = Connection::get()?;
    let query = sql::insert(M::table_name(), M::insert_columns());
    record_query_with_params(&query, &params);
    conn.insert(&query, params_from_iter(params))
}
