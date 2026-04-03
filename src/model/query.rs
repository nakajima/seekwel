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

/// A lazy query rooted at [`Query`].
pub type LazyQuery<M> = Lazy<Query<M>>;
/// A chunked query rooted at [`Query`].
pub type ChunkedQuery<M> = Chunked<Query<M>>;

use expression::QueryExpression;

/// Shared query-builder methods for query values.
///
/// Import this trait with `use seekwel::prelude::*;` to enable fluent methods
/// like `.q(...)`, `.and(...)`, `.all()`, and `.lazy()`.
pub trait QueryDsl: Sized {
    /// The persisted model type returned by this query.
    type Model: PersistedModel + 'static;
    /// The query type returned by [`QueryDsl::lazy`].
    type Lazy: QueryDsl<Model = Self::Model>;
    /// The query type returned by [`QueryDsl::chunked`].
    type Chunked: QueryDsl<Model = Self::Model>;
    /// The iterator returned by [`QueryDsl::iter`].
    type Iter: Iterator<Item = Self::IterItem>;
    /// The item yielded by [`QueryDsl::iter`].
    type IterItem;
    /// The iterator returned by [`QueryDsl::try_iter`].
    type TryIter: Iterator<Item = Self::TryIterItem>;
    /// The item yielded by [`QueryDsl::try_iter`].
    type TryIterItem;

    #[doc(hidden)]
    fn and_query(self, other: Query<Self::Model>) -> Self;

    #[doc(hidden)]
    fn or_query(self, other: Query<Self::Model>) -> Self;

    #[doc(hidden)]
    fn build_query(self, limit_one: bool) -> Result<(String, Vec<Value>), Error>;

    /// Switches this query to lazy row-by-row fetching.
    fn lazy(self) -> Self::Lazy;
    /// Switches this query to chunked fetching.
    ///
    /// Panics if `chunk_size` is `0`.
    fn chunked(self, chunk_size: usize) -> Self::Chunked;
    /// Executes the query and returns its plain iterator form.
    fn iter(self) -> Result<Self::Iter, Error>;
    /// Executes the query and returns its fallible iterator form.
    fn try_iter(self) -> Result<Self::TryIter, Error>;

    /// Adds an `AND` predicate to the current query.
    fn q<T>(self, column: <Self::Model as Model>::Column, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        self.and(Query::new(column, comparison))
    }

    /// Combines this query with another query using `AND`.
    fn and(self, other: Query<Self::Model>) -> Self {
        self.and_query(other)
    }

    /// Combines this query with another query using `OR`.
    fn or(self, other: Query<Self::Model>) -> Self {
        self.or_query(other)
    }

    /// Executes the query and returns the first matching row, if any.
    ///
    /// Without explicit ordering support, "first" means SQLite's natural result
    /// order for the generated query.
    fn first(self) -> Result<Option<Self::Model>, Error> {
        let conn = Connection::get()?;
        let (query, params) = self.build_query(true)?;
        conn.query_optional(
            &query,
            params_from_iter(params),
            <Self::Model as PersistedModel>::from_row,
        )
    }

    /// Executes the query and collects all matching rows.
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

/// Model-level query entrypoints exposed as associated functions.
///
/// Import this trait with `use seekwel::prelude::*;` to call methods like
/// `Person::all()`, `Person::lazy()`, or `Person::q(...)`.
pub trait ModelQueryDsl: PersistedModel + Sized + 'static {
    /// Starts an unfiltered query for the model.
    fn query() -> Query<Self> {
        Query::root()
    }

    /// Starts a query with a single predicate.
    fn q<T>(column: Self::Column, comparison: Comparison<T>) -> Query<Self>
    where
        T: ComparisonOperand,
    {
        Query::new(column, comparison)
    }

    /// Starts an unfiltered query and combines it with `other` using `AND`.
    fn and(other: Query<Self>) -> Query<Self> {
        <Query<Self> as QueryDsl>::and(Self::query(), other)
    }

    /// Starts an unfiltered query and combines it with `other` using `OR`.
    fn or(other: Query<Self>) -> Query<Self> {
        <Query<Self> as QueryDsl>::or(Self::query(), other)
    }

    /// Returns the first row for the model, if any.
    ///
    /// Without explicit ordering support, "first" means SQLite's natural result
    /// order for the generated query.
    fn first() -> Result<Option<Self>, Error> {
        <Query<Self> as QueryDsl>::first(Self::query())
    }

    /// Returns all rows for the model.
    fn all() -> Result<Vec<Self>, Error> {
        <Query<Self> as QueryDsl>::all(Self::query())
    }

    /// Starts a lazy unfiltered query for the model.
    fn lazy() -> Lazy<Query<Self>> {
        <Query<Self> as QueryDsl>::lazy(Self::query())
    }

    /// Starts a chunked unfiltered query for the model.
    ///
    /// Panics if `chunk_size` is `0`.
    fn chunked(chunk_size: usize) -> Chunked<Query<Self>> {
        <Query<Self> as QueryDsl>::chunked(Self::query(), chunk_size)
    }

    /// Executes an unfiltered query and returns its plain iterator form.
    fn iter() -> Result<std::vec::IntoIter<Self>, Error> {
        <Query<Self> as QueryDsl>::iter(Self::query())
    }

    /// Executes an unfiltered query and returns its fallible iterator form.
    fn try_iter() -> Result<std::vec::IntoIter<Self>, Error> {
        <Query<Self> as QueryDsl>::try_iter(Self::query())
    }
}

impl<M: PersistedModel + 'static> ModelQueryDsl for M {}

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
        sql::select(M::table_name(), M::columns(), clause.as_deref(), limit_one),
        params,
    ))
}
