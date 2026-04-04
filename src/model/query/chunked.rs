use std::marker::PhantomData;

use rusqlite::params_from_iter;

use crate::connection::Connection;
use crate::error::Error;

use super::super::PersistedModel;
use super::{Lazy, Order, Query, QueryDsl, QueryPlan, assert_chunk_size};

/// A query wrapper that fetches matching rows in chunks.
#[derive(Debug, Clone)]
pub struct Chunked<Q> {
    pub(super) inner: Q,
    pub(super) chunk_size: usize,
}

/// The fallible iterator returned by [`Chunked::try_iter`] and
/// [`QueryDsl::try_iter`] on chunked queries.
#[derive(Debug)]
pub struct ChunkedTryIter<M> {
    plan: QueryPlan,
    chunk_size: usize,
    consumed: usize,
    done: bool,
    __seekwel_model: PhantomData<M>,
}

/// The plain iterator returned by [`Chunked::iter`] and [`QueryDsl::iter`] on
/// chunked queries.
#[derive(Debug)]
pub struct ChunkedIter<M> {
    inner: ChunkedTryIter<M>,
}

#[allow(private_interfaces)]
impl<Q> QueryDsl for Chunked<Q>
where
    Q: QueryDsl,
{
    type Model = Q::Model;
    type Lazy = Lazy<Q>;
    type Chunked = Self;
    type Iter = ChunkedIter<Q::Model>;
    type IterItem = Vec<Q::Model>;
    type TryIter = ChunkedTryIter<Q::Model>;
    type TryIterItem = Result<Vec<Q::Model>, Error>;

    fn and_query(self, other: Query<Self::Model>) -> Self {
        Self {
            inner: <Q as QueryDsl>::and_query(self.inner, other),
            chunk_size: self.chunk_size,
        }
    }

    fn or_query(self, other: Query<Self::Model>) -> Self {
        Self {
            inner: <Q as QueryDsl>::or_query(self.inner, other),
            chunk_size: self.chunk_size,
        }
    }

    fn order_query(self, order: Order) -> Self {
        Self {
            inner: <Q as QueryDsl>::order_query(self.inner, order),
            chunk_size: self.chunk_size,
        }
    }

    fn limit_query(self, limit: usize) -> Self {
        Self {
            inner: <Q as QueryDsl>::limit_query(self.inner, limit),
            chunk_size: self.chunk_size,
        }
    }

    fn offset_query(self, offset: usize) -> Self {
        Self {
            inner: <Q as QueryDsl>::offset_query(self.inner, offset),
            chunk_size: self.chunk_size,
        }
    }

    fn into_query_plan(self) -> Result<QueryPlan, Error> {
        <Q as QueryDsl>::into_query_plan(self.inner)
    }

    fn lazy(self) -> Self::Lazy {
        Lazy { inner: self.inner }
    }

    fn chunked(mut self, chunk_size: usize) -> Self::Chunked {
        assert_chunk_size(chunk_size);
        self.chunk_size = chunk_size;
        self
    }

    fn iter(self) -> Result<Self::Iter, Error> {
        Ok(ChunkedIter {
            inner: <Self as QueryDsl>::try_iter(self)?,
        })
    }

    fn try_iter(self) -> Result<Self::TryIter, Error> {
        let chunk_size = self.chunk_size;
        Ok(ChunkedTryIter::new(self.into_query_plan()?, chunk_size))
    }
}

impl<Q> IntoIterator for Chunked<Q>
where
    Q: QueryDsl,
{
    type Item = Vec<Q::Model>;
    type IntoIter = ChunkedIter<Q::Model>;

    fn into_iter(self) -> Self::IntoIter {
        <Self as QueryDsl>::iter(self)
            .unwrap_or_else(|error| panic!("chunked query iteration failed to start: {error}"))
    }
}

impl<M> ChunkedTryIter<M>
where
    M: PersistedModel,
{
    fn new(plan: QueryPlan, chunk_size: usize) -> Self {
        Self {
            plan,
            chunk_size,
            consumed: 0,
            done: false,
            __seekwel_model: PhantomData,
        }
    }
}

impl<M> Iterator for ChunkedTryIter<M>
where
    M: PersistedModel,
{
    type Item = Result<Vec<M>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let remaining = self.plan.remaining_limit(self.consumed);
        if matches!(remaining, Some(0)) {
            self.done = true;
            return None;
        }

        let chunk_limit =
            remaining.map_or(self.chunk_size, |remaining| remaining.min(self.chunk_size));
        let query = self.plan.paged_query(chunk_limit, self.consumed);
        let conn = match Connection::get() {
            Ok(conn) => conn,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };

        match conn.query_all(
            &query,
            params_from_iter(self.plan.params.clone()),
            M::from_row,
        ) {
            Ok(rows) if rows.is_empty() => {
                self.done = true;
                None
            }
            Ok(rows) => {
                self.consumed += rows.len();
                self.done = rows.len() < chunk_limit
                    || matches!(self.plan.remaining_limit(self.consumed), Some(0));
                Some(Ok(rows))
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

impl<M> Iterator for ChunkedIter<M>
where
    M: PersistedModel,
{
    type Item = Vec<M>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|result| {
            result.unwrap_or_else(|error| {
                panic!("chunked query iteration failed while fetching a chunk: {error}")
            })
        })
    }
}
