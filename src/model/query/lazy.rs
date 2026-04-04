use std::marker::PhantomData;

use rusqlite::params_from_iter;

use crate::connection::Connection;
use crate::error::Error;

use super::super::PersistedModel;
use super::{Chunked, Order, Query, QueryDsl, QueryPlan, assert_chunk_size};

/// A query wrapper that fetches matching rows one at a time.
#[derive(Debug, Clone)]
pub struct Lazy<Q> {
    pub(super) inner: Q,
}

/// The fallible iterator returned by [`Lazy::try_iter`] and [`QueryDsl::try_iter`]
/// on lazy queries.
#[derive(Debug)]
pub struct LazyTryIter<M> {
    plan: QueryPlan,
    consumed: usize,
    done: bool,
    __seekwel_model: PhantomData<M>,
}

/// The plain iterator returned by [`Lazy::iter`] and [`QueryDsl::iter`] on lazy
/// queries.
#[derive(Debug)]
pub struct LazyIter<M> {
    inner: LazyTryIter<M>,
}

#[allow(private_interfaces)]
impl<Q> QueryDsl for Lazy<Q>
where
    Q: QueryDsl,
{
    type Model = Q::Model;
    type Lazy = Self;
    type Chunked = Chunked<Q>;
    type Iter = LazyIter<Q::Model>;
    type IterItem = Q::Model;
    type TryIter = LazyTryIter<Q::Model>;
    type TryIterItem = Result<Q::Model, Error>;

    fn and_query(self, other: Query<Self::Model>) -> Self {
        Self {
            inner: <Q as QueryDsl>::and_query(self.inner, other),
        }
    }

    fn or_query(self, other: Query<Self::Model>) -> Self {
        Self {
            inner: <Q as QueryDsl>::or_query(self.inner, other),
        }
    }

    fn order_query(self, order: Order) -> Self {
        Self {
            inner: <Q as QueryDsl>::order_query(self.inner, order),
        }
    }

    fn limit_query(self, limit: usize) -> Self {
        Self {
            inner: <Q as QueryDsl>::limit_query(self.inner, limit),
        }
    }

    fn offset_query(self, offset: usize) -> Self {
        Self {
            inner: <Q as QueryDsl>::offset_query(self.inner, offset),
        }
    }

    fn into_query_plan(self) -> Result<QueryPlan, Error> {
        <Q as QueryDsl>::into_query_plan(self.inner)
    }

    fn lazy(self) -> Self::Lazy {
        self
    }

    fn chunked(self, chunk_size: usize) -> Self::Chunked {
        assert_chunk_size(chunk_size);
        Chunked {
            inner: self.inner,
            chunk_size,
        }
    }

    fn iter(self) -> Result<Self::Iter, Error> {
        Ok(LazyIter {
            inner: <Self as QueryDsl>::try_iter(self)?,
        })
    }

    fn try_iter(self) -> Result<Self::TryIter, Error> {
        Ok(LazyTryIter::new(self.into_query_plan()?))
    }
}

impl<Q> IntoIterator for Lazy<Q>
where
    Q: QueryDsl,
{
    type Item = Q::Model;
    type IntoIter = LazyIter<Q::Model>;

    fn into_iter(self) -> Self::IntoIter {
        <Self as QueryDsl>::iter(self)
            .unwrap_or_else(|error| panic!("lazy query iteration failed to start: {error}"))
    }
}

impl<M> LazyTryIter<M>
where
    M: PersistedModel,
{
    fn new(plan: QueryPlan) -> Self {
        Self {
            plan,
            consumed: 0,
            done: false,
            __seekwel_model: PhantomData,
        }
    }
}

impl<M> Iterator for LazyTryIter<M>
where
    M: PersistedModel,
{
    type Item = Result<M, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if matches!(self.plan.remaining_limit(self.consumed), Some(0)) {
            self.done = true;
            return None;
        }

        let query = self.plan.paged_query(1, self.consumed);
        let conn = match Connection::get() {
            Ok(conn) => conn,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };

        match conn.query_optional(
            &query,
            params_from_iter(self.plan.params.clone()),
            M::from_row,
        ) {
            Ok(Some(model)) => {
                self.consumed += 1;
                Some(Ok(model))
            }
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

impl<M> Iterator for LazyIter<M>
where
    M: PersistedModel,
{
    type Item = M;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|result| {
            result.unwrap_or_else(|error| {
                panic!("lazy query iteration failed while fetching a row: {error}")
            })
        })
    }
}
