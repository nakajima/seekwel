use std::marker::PhantomData;

use crate::error::Error;

use super::super::{Column, Comparison, Model, PersistedModel};
use super::{Chunked, Lazy, Order, OrderTerm, QueryDsl, QueryExpression, assert_chunk_size};

/// An eager query value for a persisted model.
///
/// You usually obtain this from `Model::q(...)` or `Model::query()`.
#[derive(Debug, Clone)]
pub struct Query<M> {
    expression: QueryExpression,
    ordering: Vec<OrderTerm>,
    limit: Option<usize>,
    offset: usize,
    __seekwel_model: PhantomData<M>,
}

impl<M: Model> Query<M> {
    pub(super) fn root() -> Self {
        Self {
            expression: QueryExpression::Empty,
            ordering: Vec::new(),
            limit: None,
            offset: 0,
            __seekwel_model: PhantomData,
        }
    }

    /// Creates a query with a single predicate.
    pub fn new(column: M::Column, comparison: Comparison) -> Self {
        Self {
            expression: QueryExpression::Predicate {
                column: column.as_str(),
                comparison: comparison.into_prepared(),
            },
            ordering: Vec::new(),
            limit: None,
            offset: 0,
            __seekwel_model: PhantomData,
        }
    }
}

#[allow(private_interfaces)]
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
            ordering: self.ordering,
            limit: self.limit,
            offset: self.offset,
            __seekwel_model: PhantomData,
        }
    }

    fn or_query(self, other: Query<Self::Model>) -> Self {
        Self {
            expression: QueryExpression::Or(Box::new(self.expression), Box::new(other.expression)),
            ordering: self.ordering,
            limit: self.limit,
            offset: self.offset,
            __seekwel_model: PhantomData,
        }
    }

    fn order_query(mut self, order: Order) -> Self {
        self.ordering.extend(order.into_terms());
        self
    }

    fn limit_query(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    fn offset_query(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    fn into_query_plan(self) -> Result<super::QueryPlan, Error> {
        super::build_query_plan::<M>(self.expression, &self.ordering, self.limit, self.offset)
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
