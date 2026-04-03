use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;
use crate::sql;

use super::{Comparison, ComparisonOperand, Model, PersistedModel};

mod chunked;
mod eager;
mod expression;
mod lazy;

pub use chunked::{Chunked, ChunkedIter, ChunkedTryIter};
pub use eager::Query;
pub use lazy::{Lazy, LazyIter, LazyTryIter};

pub type LazyQuery<M> = Lazy<Query<M>>;
pub type ChunkedQuery<M> = Chunked<Query<M>>;

use expression::QueryExpression;

/// Shared query methods. Import this via `seekwel::prelude::*`.
pub trait QueryDsl: Sized {
    type Model: PersistedModel + 'static;
    type Lazy: QueryDsl<Model = Self::Model>;
    type Chunked: QueryDsl<Model = Self::Model>;
    type Iter: Iterator<Item = Self::IterItem>;
    type IterItem;
    type TryIter: Iterator<Item = Self::TryIterItem>;
    type TryIterItem;

    #[doc(hidden)]
    fn and_query(self, other: Query<Self::Model>) -> Self;

    #[doc(hidden)]
    fn or_query(self, other: Query<Self::Model>) -> Self;

    #[doc(hidden)]
    fn build_query(self, limit_one: bool) -> Result<(String, Vec<Value>), Error>;

    fn lazy(self) -> Self::Lazy;
    fn chunked(self, chunk_size: usize) -> Self::Chunked;
    fn iter(self) -> Result<Self::Iter, Error>;
    fn try_iter(self) -> Result<Self::TryIter, Error>;

    fn q<T>(self, column: <Self::Model as Model>::Column, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        self.and(Query::new(column, comparison))
    }

    fn and(self, other: Query<Self::Model>) -> Self {
        self.and_query(other)
    }

    fn or(self, other: Query<Self::Model>) -> Self {
        self.or_query(other)
    }

    fn first(self) -> Result<Option<Self::Model>, Error> {
        let conn = Connection::get()?;
        let (query, params) = self.build_query(true)?;
        conn.query_optional(
            &query,
            params_from_iter(params),
            <Self::Model as PersistedModel>::from_row,
        )
    }

    fn all(self) -> Result<Vec<Self::Model>, Error> {
        let conn = Connection::get()?;
        let (query, params) = self.build_query(false)?;
        conn.query_all(
            &query,
            params_from_iter(params),
            <Self::Model as PersistedModel>::from_row,
        )
    }
}

fn assert_chunk_size(chunk_size: usize) {
    assert!(chunk_size > 0, "chunk size must be greater than 0");
}

fn build_query<M: Model>(
    expression: QueryExpression,
    limit_one: bool,
) -> Result<(String, Vec<Value>), Error> {
    let mut params = Vec::new();
    let clause = expression.into_clause(&mut params)?;

    Ok((
        sql::select_where(M::table_name(), M::columns(), &clause, limit_one),
        params,
    ))
}
