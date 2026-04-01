use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;
use crate::sql;

#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: &'static str,
    pub sql_type: &'static str,
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NewRecord;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Persisted;

pub trait Model: Sized {
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

    fn reload(self) -> Result<Self, Error> {
        let conn = Connection::get()?;
        conn.query_row(
            &sql::select_by_id(Self::table_name(), Self::columns()),
            [self.id() as i64],
            Self::from_row,
        )
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
