use std::marker::PhantomData;

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;

use super::super::PersistedModel;
use super::{Chunked, Query, QueryDsl, assert_chunk_size};

#[derive(Debug, Clone)]
pub struct Lazy<Q> {
    pub(super) inner: Q,
}

#[derive(Debug)]
pub struct LazyTryIter<M> {
    query: String,
    params: Vec<Value>,
    offset: usize,
    done: bool,
    __seekwel_model: PhantomData<M>,
}

#[derive(Debug)]
pub struct LazyIter<M> {
    inner: LazyTryIter<M>,
}

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

    fn build_query(self, limit_one: bool) -> Result<(String, Vec<Value>), Error> {
        <Q as QueryDsl>::build_query(self.inner, limit_one)
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
        let (query, params) = <Self as QueryDsl>::build_query(self, false)?;
        Ok(LazyTryIter::new(query, params))
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
    fn new(query: String, params: Vec<Value>) -> Self {
        Self {
            query,
            params,
            offset: 0,
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

        let query = format!("{} LIMIT 1 OFFSET {}", self.query, self.offset);
        let conn = match Connection::get() {
            Ok(conn) => conn,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };

        match conn.query_optional(&query, params_from_iter(self.params.clone()), M::from_row) {
            Ok(Some(model)) => {
                self.offset += 1;
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
