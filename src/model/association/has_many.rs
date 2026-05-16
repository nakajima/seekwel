use std::fmt;
use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard};

use crate::error::Error;
use crate::model::{Chunked, Comparison, Lazy, Model, Order, PersistedModel, Query, QueryDsl};

/// Internal child-side metadata shared by all [`HasMany`] associations for a model.
#[doc(hidden)]
pub trait HasManyChild: PersistedModel + Clone + Sized + 'static {
    type Builder;
}

/// Runtime handlers for a specific generated [`HasMany`] foreign key.
#[doc(hidden)]
pub struct HasManyHandlers<Child: HasManyChild, Parent = ()> {
    query_for_parent: fn(u64) -> Result<Query<Child>, Error>,
    append_for_parent: fn(u64, <Child as HasManyChild>::Builder) -> Result<Child, Error>,
    __seekwel_parent: PhantomData<fn() -> Parent>,
}

impl<Child: HasManyChild, Parent> HasManyHandlers<Child, Parent> {
    #[doc(hidden)]
    pub fn new(
        query_for_parent: fn(u64) -> Result<Query<Child>, Error>,
        append_for_parent: fn(u64, <Child as HasManyChild>::Builder) -> Result<Child, Error>,
    ) -> Self {
        Self {
            query_for_parent,
            append_for_parent,
            __seekwel_parent: PhantomData,
        }
    }

    fn erase_parent(self) -> HasManyHandlers<Child> {
        HasManyHandlers::new(self.query_for_parent, self.append_for_parent)
    }
}

impl<Child: HasManyChild> HasManyHandlers<Child> {
    fn missing() -> Self {
        Self::new(
            missing_query_for_parent::<Child>,
            missing_append_for_parent::<Child>,
        )
    }
}

impl<Child: HasManyChild, Parent> Clone for HasManyHandlers<Child, Parent> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Child: HasManyChild, Parent> Copy for HasManyHandlers<Child, Parent> {}

fn missing_query_for_parent<Child: HasManyChild>(_parent_id: u64) -> Result<Query<Child>, Error> {
    Err(Error::InvalidAssociation(
        "HasMany association was not initialized by seekwel::model".to_string(),
    ))
}

fn missing_append_for_parent<Child: HasManyChild>(
    _parent_id: u64,
    _builder: <Child as HasManyChild>::Builder,
) -> Result<Child, Error> {
    Err(Error::InvalidAssociation(
        "HasMany association was not initialized by seekwel::model".to_string(),
    ))
}

/// A reverse association to many child records through a child's `BelongsTo` field.
///
/// Values are usually created by `#[seekwel::model]` and stored on persisted
/// parent records. They are bound to the parent record's `id`, can lazily load
/// child rows, can query child rows, and can append new children through the
/// generated child builder.
pub struct HasMany<Child: HasManyChild> {
    parent_id: Option<u64>,
    handlers: HasManyHandlers<Child>,
    cached: Mutex<Option<Vec<Child>>>,
}

impl<Child: HasManyChild> HasMany<Child> {
    /// Creates an unbound association for an unsaved parent record.
    pub fn new_unbound() -> Self {
        Self::new_unbound_with_handlers(HasManyHandlers::missing())
    }

    /// Creates an association bound to a persisted parent id.
    pub fn new_bound(parent_id: u64) -> Self {
        Self::new_bound_with_handlers(parent_id, HasManyHandlers::missing())
    }

    #[doc(hidden)]
    pub fn new_unbound_with_handlers<Parent>(handlers: HasManyHandlers<Child, Parent>) -> Self {
        Self {
            parent_id: None,
            handlers: handlers.erase_parent(),
            cached: Mutex::new(None),
        }
    }

    #[doc(hidden)]
    pub fn new_bound_with_handlers<Parent>(
        parent_id: u64,
        handlers: HasManyHandlers<Child, Parent>,
    ) -> Self {
        Self {
            parent_id: Some(parent_id),
            handlers: handlers.erase_parent(),
            cached: Mutex::new(None),
        }
    }

    /// Returns the bound parent id, if any.
    pub fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    /// Clears any cached child records for this association.
    pub fn clear_cache(&self) {
        self.cached().take();
    }

    fn cached(&self) -> MutexGuard<'_, Option<Vec<Child>>> {
        self.cached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn require_parent_id(&self) -> Result<u64, Error> {
        self.parent_id.ok_or_else(|| {
            Error::InvalidAssociation(
                "HasMany association is not bound to a persisted parent record".to_string(),
            )
        })
    }

    fn query_for_parent(&self) -> Result<Query<Child>, Error> {
        (self.handlers.query_for_parent)(self.require_parent_id()?)
    }

    /// Starts a query scoped to this association's bound parent.
    pub fn query(&self) -> HasManyQuery<Child> {
        HasManyQuery::new(self.query_for_parent())
    }

    /// Starts an association-scoped query with a single predicate.
    pub fn q(
        &self,
        column: <Child as Model>::Column,
        comparison: Comparison,
    ) -> HasManyQuery<Child> {
        self.query().q(column, comparison)
    }

    /// Starts an association-scoped query and combines its filter with `other` using `AND`.
    pub fn and(&self, other: Query<Child>) -> HasManyQuery<Child> {
        self.query().and(other)
    }

    /// Starts an association-scoped query and combines its filter with `other` using `OR`.
    ///
    /// The parent association constraint is still applied to the combined filter.
    pub fn or(&self, other: Query<Child>) -> HasManyQuery<Child> {
        self.query().or(other)
    }

    /// Starts an association-scoped ordered query.
    pub fn order<O>(&self, order: O) -> HasManyQuery<Child>
    where
        O: Into<Order>,
    {
        self.query().order(order)
    }

    /// Starts an association-scoped ascending ordered query for the given column.
    pub fn order_by(&self, column: <Child as Model>::Column) -> HasManyQuery<Child> {
        self.query().order_by(column)
    }

    /// Starts an association-scoped ascending ordered query for the given column.
    pub fn asc(&self, column: <Child as Model>::Column) -> HasManyQuery<Child> {
        self.query().asc(column)
    }

    /// Starts an association-scoped descending ordered query for the given column.
    pub fn desc(&self, column: <Child as Model>::Column) -> HasManyQuery<Child> {
        self.query().desc(column)
    }

    /// Starts an association-scoped limited query.
    pub fn limit(&self, limit: usize) -> HasManyQuery<Child> {
        self.query().limit(limit)
    }

    /// Starts an association-scoped offset query.
    pub fn offset(&self, offset: usize) -> HasManyQuery<Child> {
        self.query().offset(offset)
    }

    /// Returns the first child row for this association, if any.
    pub fn first(&self) -> Result<Option<Child>, Error> {
        self.query().first()
    }

    /// Counts how many child rows this association query would return.
    pub fn count(&self) -> Result<usize, Error> {
        self.query().count()
    }

    /// Returns whether this association query would yield at least one row.
    pub fn exists(&self) -> Result<bool, Error> {
        self.query().exists()
    }

    /// Executes an association-scoped query and collects all matching child rows.
    pub fn all(&self) -> Result<Vec<Child>, Error> {
        self.query().all()
    }

    /// Starts a lazy association-scoped query.
    pub fn lazy(&self) -> Lazy<HasManyQuery<Child>> {
        self.query().lazy()
    }

    /// Starts a chunked association-scoped query.
    ///
    /// Panics if `chunk_size` is `0`.
    pub fn chunked(&self, chunk_size: usize) -> Chunked<HasManyQuery<Child>> {
        self.query().chunked(chunk_size)
    }

    /// Executes an association-scoped query and returns its plain iterator form.
    pub fn iter(&self) -> Result<std::vec::IntoIter<Child>, Error> {
        self.query().iter()
    }

    /// Executes an association-scoped query and returns its fallible iterator form.
    pub fn try_iter(&self) -> Result<std::vec::IntoIter<Child>, Error> {
        self.query().try_iter()
    }

    /// Loads all child records for the bound parent, caching them on first access.
    pub fn load(&self) -> Result<Vec<Child>, Error> {
        {
            let cached = self.cached();
            if let Some(children) = cached.as_ref() {
                return Ok(children.clone());
            }
        }

        let children = self.query().all()?;
        *self.cached() = Some(children.clone());
        Ok(children)
    }

    /// Creates and associates a new child record through the generated child builder.
    pub fn append(&self, builder: <Child as HasManyChild>::Builder) -> Result<Child, Error> {
        let child = (self.handlers.append_for_parent)(self.require_parent_id()?, builder)?;

        let mut cached = self.cached();
        if let Some(children) = cached.as_mut() {
            children.push(child.clone());
        }

        Ok(child)
    }
}

impl<Child: HasManyChild> Default for HasMany<Child> {
    fn default() -> Self {
        Self::new_unbound()
    }
}

impl<Child: HasManyChild> Clone for HasMany<Child> {
    fn clone(&self) -> Self {
        match self.parent_id {
            Some(parent_id) => Self::new_bound_with_handlers(parent_id, self.handlers),
            None => Self::new_unbound_with_handlers(self.handlers),
        }
    }
}

impl<Child: HasManyChild> fmt::Debug for HasMany<Child> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HasMany")
            .field("parent_id", &self.parent_id)
            .finish()
    }
}

impl<Child: HasManyChild> PartialEq for HasMany<Child> {
    fn eq(&self, other: &Self) -> bool {
        self.parent_id == other.parent_id
    }
}

impl<Child: HasManyChild> Eq for HasMany<Child> {}

/// A query value scoped to a [`HasMany`] association.
///
/// It supports the same [`QueryDsl`] methods as model-level queries while
/// keeping the association's parent constraint applied to every execution.
#[derive(Debug)]
pub struct HasManyQuery<Child: HasManyChild> {
    scope: Result<Query<Child>, Error>,
    filters: Option<Query<Child>>,
}

impl<Child: HasManyChild> HasManyQuery<Child> {
    fn new(scope: Result<Query<Child>, Error>) -> Self {
        Self {
            scope,
            filters: None,
        }
    }
}

#[allow(private_interfaces)]
impl<Child: HasManyChild> QueryDsl for HasManyQuery<Child> {
    type Model = Child;
    type Lazy = Lazy<Self>;
    type Chunked = Chunked<Self>;
    type Iter = std::vec::IntoIter<Child>;
    type IterItem = Child;
    type TryIter = std::vec::IntoIter<Child>;
    type TryIterItem = Child;

    fn and_query(self, other: Query<Self::Model>) -> Self {
        let filters = Some(match self.filters {
            Some(filters) => <Query<Child> as QueryDsl>::and_query(filters, other),
            None => other,
        });

        Self {
            scope: self.scope,
            filters,
        }
    }

    fn or_query(self, other: Query<Self::Model>) -> Self {
        let filters = Some(match self.filters {
            Some(filters) => <Query<Child> as QueryDsl>::or_query(filters, other),
            None => other,
        });

        Self {
            scope: self.scope,
            filters,
        }
    }

    fn order_query(self, order: Order) -> Self {
        Self {
            scope: self
                .scope
                .map(|scope| <Query<Child> as QueryDsl>::order_query(scope, order)),
            filters: self.filters,
        }
    }

    fn limit_query(self, limit: usize) -> Self {
        Self {
            scope: self
                .scope
                .map(|scope| <Query<Child> as QueryDsl>::limit_query(scope, limit)),
            filters: self.filters,
        }
    }

    fn offset_query(self, offset: usize) -> Self {
        Self {
            scope: self
                .scope
                .map(|scope| <Query<Child> as QueryDsl>::offset_query(scope, offset)),
            filters: self.filters,
        }
    }

    fn into_query_plan(self) -> Result<super::super::query::QueryPlan, Error> {
        let scope = self.scope?;
        let query = match self.filters {
            Some(filters) => <Query<Child> as QueryDsl>::and_query(scope, filters),
            None => scope,
        };

        <Query<Child> as QueryDsl>::into_query_plan(query)
    }

    fn lazy(self) -> Self::Lazy {
        Lazy::new(self)
    }

    fn chunked(self, chunk_size: usize) -> Self::Chunked {
        super::super::query::assert_chunk_size(chunk_size);
        Chunked::new(self, chunk_size)
    }

    fn iter(self) -> Result<Self::Iter, Error> {
        Ok(<Self as QueryDsl>::all(self)?.into_iter())
    }

    fn try_iter(self) -> Result<Self::TryIter, Error> {
        self.iter()
    }
}

impl<Child: HasManyChild> IntoIterator for HasManyQuery<Child> {
    type Item = Child;
    type IntoIter = std::vec::IntoIter<Child>;

    fn into_iter(self) -> Self::IntoIter {
        <Self as QueryDsl>::iter(self)
            .unwrap_or_else(|error| panic!("has_many query iteration failed to start: {error}"))
    }
}
