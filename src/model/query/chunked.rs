use std::marker::PhantomData;

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;

use super::super::PersistedModel;
use super::{Lazy, Query, QueryDsl, assert_chunk_size};

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
    query: String,
    params: Vec<Value>,
    chunk_size: usize,
    offset: usize,
    done: bool,
    __seekwel_model: PhantomData<M>,
}

/// The plain iterator returned by [`Chunked::iter`] and [`QueryDsl::iter`] on
/// chunked queries.
#[derive(Debug)]
pub struct ChunkedIter<M> {
    inner: ChunkedTryIter<M>,
}

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

    fn build_query(self, limit_one: bool) -> Result<(String, Vec<Value>), Error> {
        <Q as QueryDsl>::build_query(self.inner, limit_one)
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
        let (query, params) = <Q as QueryDsl>::build_query(self.inner, false)?;
        Ok(ChunkedTryIter::new(query, params, chunk_size))
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
    fn new(query: String, params: Vec<Value>, chunk_size: usize) -> Self {
        Self {
            query,
            params,
            chunk_size,
            offset: 0,
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

        let query = format!(
            "{} LIMIT {} OFFSET {}",
            self.query, self.chunk_size, self.offset
        );
        let conn = match Connection::get() {
            Ok(conn) => conn,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };

        match conn.query_all(&query, params_from_iter(self.params.clone()), M::from_row) {
            Ok(rows) if rows.is_empty() => {
                self.done = true;
                None
            }
            Ok(rows) => {
                self.offset += rows.len();
                self.done = rows.len() < self.chunk_size;
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
