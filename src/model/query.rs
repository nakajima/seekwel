use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::{Connection, record_query_with_params};
use crate::error::Error;
use crate::sql::{self, Count, OrderDirection, OrderTerm, Projection, Select};

use super::{Column, Comparison, Model, PersistedModel};

mod chunked;
mod eager;
mod expression;
mod lazy;

pub use chunked::{Chunked, ChunkedIter, ChunkedTryIter};
pub use eager::Query;
pub use lazy::{Lazy, LazyIter, LazyTryIter};

/// An `ORDER BY` clause builder.
///
/// It can be constructed from:
/// - a typed column like `PersonColumns::Name` (defaults to ascending)
/// - `PersonColumns::Name.asc()` or `PersonColumns::Name.desc()`
/// - arrays like `[PersonColumns::Name, PersonColumns::Age]` or
///   `[PersonColumns::Name.asc(), PersonColumns::Age.desc()]`
/// - raw SQL fragments like `"name DESC"`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Order {
    terms: Vec<OrderTerm>,
}

impl Order {
    /// Creates an ascending `ORDER BY` item for a typed column.
    pub fn asc<C>(column: C) -> Self
    where
        C: Column,
    {
        Self {
            terms: vec![OrderTerm::Column {
                name: column.as_str(),
                direction: OrderDirection::Asc,
            }],
        }
    }

    /// Creates a descending `ORDER BY` item for a typed column.
    pub fn desc<C>(column: C) -> Self
    where
        C: Column,
    {
        Self {
            terms: vec![OrderTerm::Column {
                name: column.as_str(),
                direction: OrderDirection::Desc,
            }],
        }
    }

    /// Creates a raw `ORDER BY` fragment.
    ///
    /// This is inserted into SQL as-is.
    pub fn raw(sql: impl Into<String>) -> Self {
        Self {
            terms: vec![OrderTerm::Raw(sql.into())],
        }
    }

    /// Appends another ordering onto this one.
    pub fn then(mut self, other: impl Into<Order>) -> Self {
        self.terms.extend(other.into().terms);
        self
    }

    pub(super) fn into_terms(self) -> Vec<OrderTerm> {
        self.terms
    }
}

impl<C> From<C> for Order
where
    C: Column,
{
    fn from(column: C) -> Self {
        Self::asc(column)
    }
}

impl From<&str> for Order {
    fn from(sql: &str) -> Self {
        Self::raw(sql)
    }
}

impl From<String> for Order {
    fn from(sql: String) -> Self {
        Self::raw(sql)
    }
}

impl From<&String> for Order {
    fn from(sql: &String) -> Self {
        Self::raw(sql.clone())
    }
}

impl<T, const N: usize> From<[T; N]> for Order
where
    T: Into<Order>,
{
    fn from(values: [T; N]) -> Self {
        let mut order = Self::default();
        for value in values {
            order.terms.extend(value.into().terms);
        }
        order
    }
}

impl<T> From<Vec<T>> for Order
where
    T: Into<Order>,
{
    fn from(values: Vec<T>) -> Self {
        let mut order = Self::default();
        for value in values {
            order.terms.extend(value.into().terms);
        }
        order
    }
}

/// A lazy query rooted at [`Query`].
pub type LazyQuery<M> = Lazy<Query<M>>;
/// A chunked query rooted at [`Query`].
pub type ChunkedQuery<M> = Chunked<Query<M>>;

use expression::QueryExpression;

#[derive(Debug, Clone)]
pub(super) struct QueryPlan {
    table_name: &'static str,
    primary_key_name: &'static str,
    columns: &'static [super::ColumnDef],
    clause: Option<String>,
    order_clause: Option<String>,
    pub(super) params: Vec<Value>,
    limit: Option<usize>,
    offset: usize,
}

impl QueryPlan {
    fn select<'a>(
        &'a self,
        projection: Projection<'a>,
        limit: Option<usize>,
        offset: usize,
    ) -> Select<'a> {
        Select {
            projection,
            table_name: self.table_name,
            clause: self.clause.as_deref(),
            order_clause: self.order_clause.as_deref(),
            limit,
            offset: (offset > 0).then_some(offset),
        }
    }

    pub(super) fn all_query(&self) -> String {
        self.select(
            Projection::ModelColumns {
                primary_key_name: self.primary_key_name,
                columns: self.columns,
            },
            self.limit,
            self.offset,
        )
        .to_sql()
    }

    pub(super) fn first_query(&self) -> String {
        let limit = match self.limit {
            Some(limit) => Some(limit.min(1)),
            None => Some(1),
        };

        self.select(
            Projection::ModelColumns {
                primary_key_name: self.primary_key_name,
                columns: self.columns,
            },
            limit,
            self.offset,
        )
            .to_sql()
    }

    pub(super) fn count_query(&self) -> String {
        Count {
            select: self.select(Projection::One, self.limit, self.offset),
        }
        .to_sql()
    }

    pub(super) fn exists_query(&self) -> String {
        let limit = match self.limit {
            Some(limit) => Some(limit.min(1)),
            None => Some(1),
        };

        self.select(Projection::One, limit, self.offset).to_sql()
    }

    pub(super) fn remaining_limit(&self, consumed: usize) -> Option<usize> {
        self.limit.map(|limit| limit.saturating_sub(consumed))
    }

    pub(super) fn paged_query(&self, page_size: usize, consumed: usize) -> String {
        let limit = match self.remaining_limit(consumed) {
            Some(limit) => Some(limit.min(page_size)),
            None => Some(page_size),
        };
        let offset = self.offset.saturating_add(consumed);

        self.select(
            Projection::ModelColumns {
                primary_key_name: self.primary_key_name,
                columns: self.columns,
            },
            limit,
            offset,
        )
            .to_sql()
    }
}

/// Shared query-builder methods for query values.
///
/// Import this trait with `use seekwel::prelude::*;` to enable fluent methods
/// like `.q(...)`, `.and(...)`, `.order(...)`, `.count()`, `.exists()`, and `.all()`.
#[allow(private_interfaces)]
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
    fn order_query(self, order: Order) -> Self;

    #[doc(hidden)]
    fn limit_query(self, limit: usize) -> Self;

    #[doc(hidden)]
    fn offset_query(self, offset: usize) -> Self;

    #[doc(hidden)]
    fn into_query_plan(self) -> Result<QueryPlan, Error>;

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
    fn q(self, column: <Self::Model as Model>::Column, comparison: Comparison) -> Self {
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

    /// Adds one or more `ORDER BY` items.
    fn order<O>(self, order: O) -> Self
    where
        O: Into<Order>,
    {
        self.order_query(order.into())
    }

    /// Adds an ascending `ORDER BY` clause for the given column.
    fn order_by(self, column: <Self::Model as Model>::Column) -> Self {
        self.order(column)
    }

    /// Adds an ascending `ORDER BY` clause for the given column.
    fn asc(self, column: <Self::Model as Model>::Column) -> Self {
        self.order(Order::asc(column))
    }

    /// Adds a descending `ORDER BY` clause for the given column.
    fn desc(self, column: <Self::Model as Model>::Column) -> Self {
        self.order(Order::desc(column))
    }

    /// Limits the number of rows returned by this query.
    fn limit(self, limit: usize) -> Self {
        self.limit_query(limit)
    }

    /// Skips the first `offset` rows returned by this query.
    fn offset(self, offset: usize) -> Self {
        self.offset_query(offset)
    }

    /// Executes the query and returns the first matching row, if any.
    fn first(self) -> Result<Option<Self::Model>, Error> {
        let conn = Connection::get()?;
        let plan = self.into_query_plan()?;
        let query = plan.first_query();
        record_query_with_params(&query, &plan.params);
        conn.query_optional(
            &query,
            params_from_iter(plan.params),
            <Self::Model as PersistedModel>::from_row,
        )
    }

    /// Counts how many rows this query would return.
    fn count(self) -> Result<usize, Error> {
        let conn = Connection::get()?;
        let plan = self.into_query_plan()?;
        let query = plan.count_query();
        record_query_with_params(&query, &plan.params);
        let count = conn.query_row(&query, params_from_iter(plan.params), |row| {
            row.get::<_, i64>(0)
        })?;
        Ok(count as usize)
    }

    /// Returns whether this query would yield at least one row.
    fn exists(self) -> Result<bool, Error> {
        let conn = Connection::get()?;
        let plan = self.into_query_plan()?;
        let query = plan.exists_query();
        record_query_with_params(&query, &plan.params);
        let value = conn.query_optional(&query, params_from_iter(plan.params), |row| {
            row.get::<_, i64>(0)
        })?;
        Ok(value.is_some())
    }

    /// Executes the query and collects all matching rows.
    fn all(self) -> Result<Vec<Self::Model>, Error> {
        let conn = Connection::get()?;
        let plan = self.into_query_plan()?;
        let query = plan.all_query();
        record_query_with_params(&query, &plan.params);
        conn.query_all(
            &query,
            params_from_iter(plan.params),
            <Self::Model as PersistedModel>::from_row,
        )
    }
}

/// Model-level query entrypoints exposed as associated functions.
///
/// Import this trait with `use seekwel::prelude::*;` to call methods like
/// `Person::all()`, `Person::count()`, `Person::exists()`, `Person::q(...)`, or `Person::order(...)`.
pub trait ModelQueryDsl: PersistedModel + Sized + 'static {
    /// Starts an unfiltered query for the model.
    fn query() -> Query<Self> {
        Query::root()
    }

    /// Starts a query with a single predicate.
    fn q(column: Self::Column, comparison: Comparison) -> Query<Self> {
        Query::new(column, comparison)
    }

    /// Starts an unfiltered query and combines it with `other` using `AND`.
    fn and(other: Query<Self>) -> Query<Self> {
        other
    }

    /// Starts an unfiltered query and combines it with `other` using `OR`.
    fn or(other: Query<Self>) -> Query<Self> {
        other
    }

    /// Starts an unfiltered ordered query for the model.
    fn order<O>(order: O) -> Query<Self>
    where
        O: Into<Order>,
    {
        <Query<Self> as QueryDsl>::order(Self::query(), order)
    }

    /// Starts an unfiltered ascending ordered query for the model.
    fn order_by(column: Self::Column) -> Query<Self> {
        <Query<Self> as QueryDsl>::order_by(Self::query(), column)
    }

    /// Starts an unfiltered ascending ordered query for the model.
    fn asc(column: Self::Column) -> Query<Self> {
        <Query<Self> as QueryDsl>::asc(Self::query(), column)
    }

    /// Starts an unfiltered descending ordered query for the model.
    fn desc(column: Self::Column) -> Query<Self> {
        <Query<Self> as QueryDsl>::desc(Self::query(), column)
    }

    /// Starts an unfiltered limited query for the model.
    fn limit(limit: usize) -> Query<Self> {
        <Query<Self> as QueryDsl>::limit(Self::query(), limit)
    }

    /// Starts an unfiltered offset query for the model.
    fn offset(offset: usize) -> Query<Self> {
        <Query<Self> as QueryDsl>::offset(Self::query(), offset)
    }

    /// Returns the first row for the model, if any.
    fn first() -> Result<Option<Self>, Error> {
        <Query<Self> as QueryDsl>::first(Self::query())
    }

    /// Counts how many rows the model query would return.
    fn count() -> Result<usize, Error> {
        <Query<Self> as QueryDsl>::count(Self::query())
    }

    /// Returns whether the model query would yield at least one row.
    fn exists() -> Result<bool, Error> {
        <Query<Self> as QueryDsl>::exists(Self::query())
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

fn build_query_plan<M: Model>(
    expression: QueryExpression,
    ordering: &[OrderTerm],
    limit: Option<usize>,
    offset: usize,
) -> Result<QueryPlan, Error> {
    let mut params = Vec::new();
    let clause = expression.into_clause(&mut params)?;
    let order_clause = sql::order_by_clause(ordering);

    Ok(QueryPlan {
        table_name: M::table_name(),
        primary_key_name: M::primary_key().name,
        columns: M::columns(),
        clause,
        order_clause,
        params,
        limit,
        offset,
    })
}
