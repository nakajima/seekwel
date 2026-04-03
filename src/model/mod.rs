use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;
use crate::sql;

pub mod builder;
mod comparison;
mod query;
mod sql_field;

pub use comparison::{Comparison, ComparisonOperand};
pub use query::{
    Chunked, ChunkedIter, ChunkedQuery, ChunkedTryIter, Lazy, LazyIter, LazyQuery, LazyTryIter,
    Query, QueryDsl,
};
pub use sql_field::SqlField;
#[doc(hidden)]
pub use sql_field::column;

#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: &'static str,
    pub sql_type: &'static str,
    pub nullable: bool,
}

pub trait Column: Copy + Clone {
    fn as_str(self) -> &'static str;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NewRecord;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Persisted;

pub trait Model: Sized {
    type Column: Column;

    fn table_name() -> &'static str;
    fn columns() -> &'static [ColumnDef];
    /// Returns values for non-id columns, in the same order as `columns()`.
    fn params(&self) -> Vec<Value>;

    fn create_table() -> Result<(), Error> {
        let conn = Connection::get()?;
        conn.execute(&sql::create_table(Self::table_name(), Self::columns()), ())?;
        Ok(())
    }
}

pub trait PersistedModel: Model + Sized {
    fn id(&self) -> u64;
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>;

    fn find(id: u64) -> Result<Self, Error> {
        let conn = Connection::get()?;
        conn.query_row(
            &sql::select_by_id(Self::table_name(), Self::columns()),
            [id as i64],
            Self::from_row,
        )
    }

    fn reload(&mut self) -> Result<(), Error> {
        *self = Self::find(self.id())?;
        Ok(())
    }
}

pub fn insert<M: Model>(model: &M) -> Result<u64, Error> {
    let params = model.params();
    let conn = Connection::get()?;
    conn.insert(
        &sql::insert(M::table_name(), M::columns()),
        params_from_iter(params),
    )
}
