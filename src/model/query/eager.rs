use std::marker::PhantomData;

use crate::error::Error;

use super::super::{Column, Comparison, ComparisonOperand, Model, PersistedModel};
use super::{Chunked, Lazy, QueryDsl, QueryExpression, assert_chunk_size};

#[derive(Debug, Clone)]
pub struct Query<M> {
    expression: QueryExpression,
    __seekwel_model: PhantomData<M>,
}

impl<M: Model> Query<M> {
    pub fn new<T>(column: M::Column, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        Self {
            expression: QueryExpression::Predicate {
                column: column.as_str(),
                comparison: comparison.into_prepared(),
            },
            __seekwel_model: PhantomData,
        }
    }
}

impl<M: PersistedModel + 'static> QueryDsl for Query<M> {
    type Model = M;
    type Lazy = Lazy<Self>;
    type Chunked = Chunked<Self>;
    type Iter = std::vec::IntoIter<M>;
    type IterItem = M;
    type TryIter = std::vec::IntoIter<M>;
    type TryIterItem = M;

    fn and_query(self, other: Query<Self::Model>) -> Self {
        Self {
            expression: QueryExpression::And(Box::new(self.expression), Box::new(other.expression)),
            __seekwel_model: PhantomData,
        }
    }

    fn or_query(self, other: Query<Self::Model>) -> Self {
        Self {
            expression: QueryExpression::Or(Box::new(self.expression), Box::new(other.expression)),
            __seekwel_model: PhantomData,
        }
    }

    fn build_query(self, limit_one: bool) -> Result<(String, Vec<rusqlite::types::Value>), Error> {
        super::build_query::<M>(self.expression, limit_one)
    }

    fn lazy(self) -> Self::Lazy {
        Lazy { inner: self }
    }

    fn chunked(self, chunk_size: usize) -> Self::Chunked {
        assert_chunk_size(chunk_size);
        Chunked {
            inner: self,
            chunk_size,
        }
    }

    fn iter(self) -> Result<Self::Iter, Error> {
        Ok(<Self as QueryDsl>::all(self)?.into_iter())
    }

    fn try_iter(self) -> Result<Self::TryIter, Error> {
        self.iter()
    }
}

impl<M: PersistedModel + 'static> IntoIterator for Query<M> {
    type Item = M;
    type IntoIter = std::vec::IntoIter<M>;

    fn into_iter(self) -> Self::IntoIter {
        <Self as QueryDsl>::iter(self)
            .unwrap_or_else(|error| panic!("query iteration failed to start: {error}"))
    }
}
