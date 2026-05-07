use std::cell::RefCell;
use std::fmt;

use crate::error::Error;
use crate::model::{Model, PersistedModel};

/// Internal child-side association metadata used by [`HasMany`].
#[doc(hidden)]
pub trait HasManyAssociation<const ASSOC: u8>: PersistedModel + Clone + Sized {
    type Parent;
    type Builder;

    fn load_for_parent(parent_id: u64) -> Result<Vec<Self>, Error>;
    fn append_for_parent(parent_id: u64, builder: Self::Builder) -> Result<Self, Error>;
}

/// A reverse association to many child records through a child's `BelongsTo` field.
///
/// Values are usually created by `#[seekwel::model]` and stored on persisted
/// parent records. They are bound to the parent record's `id`, can lazily load
/// child rows, and can append new children through the generated child builder.
pub struct HasMany<Child: Model, const ASSOC: u8> {
    parent_id: Option<u64>,
    cached: RefCell<Option<Vec<Child>>>,
}

impl<Child: Model, const ASSOC: u8> HasMany<Child, ASSOC> {
    /// Creates an unbound association for an unsaved parent record.
    pub fn new_unbound() -> Self {
        Self {
            parent_id: None,
            cached: RefCell::new(None),
        }
    }

    /// Creates an association bound to a persisted parent id.
    pub fn new_bound(parent_id: u64) -> Self {
        Self {
            parent_id: Some(parent_id),
            cached: RefCell::new(None),
        }
    }

    /// Returns the bound parent id, if any.
    pub fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    /// Clears any cached child records for this association.
    pub fn clear_cache(&self) {
        self.cached.borrow_mut().take();
    }

    fn require_parent_id(&self) -> Result<u64, Error> {
        self.parent_id.ok_or_else(|| {
            Error::InvalidAssociation(
                "HasMany association is not bound to a persisted parent record".to_string(),
            )
        })
    }
}

impl<Child, const ASSOC: u8> HasMany<Child, ASSOC>
where
    Child: HasManyAssociation<ASSOC>,
{
    /// Loads all child records for the bound parent, caching them on first access.
    pub fn load(&self) -> Result<Vec<Child>, Error> {
        if let Some(cached) = self.cached.borrow().as_ref() {
            return Ok(cached.clone());
        }

        let children = Child::load_for_parent(self.require_parent_id()?)?;
        *self.cached.borrow_mut() = Some(children.clone());
        Ok(children)
    }

    /// Creates and associates a new child record through the generated child builder.
    pub fn append(
        &self,
        builder: <Child as HasManyAssociation<ASSOC>>::Builder,
    ) -> Result<Child, Error> {
        let child = Child::append_for_parent(self.require_parent_id()?, builder)?;

        if let Some(cached) = self.cached.borrow_mut().as_mut() {
            cached.push(child.clone());
        }

        Ok(child)
    }
}

impl<Child: Model, const ASSOC: u8> Default for HasMany<Child, ASSOC> {
    fn default() -> Self {
        Self::new_unbound()
    }
}

impl<Child: Model, const ASSOC: u8> Clone for HasMany<Child, ASSOC> {
    fn clone(&self) -> Self {
        match self.parent_id {
            Some(parent_id) => Self::new_bound(parent_id),
            None => Self::new_unbound(),
        }
    }
}

impl<Child: Model, const ASSOC: u8> fmt::Debug for HasMany<Child, ASSOC> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HasMany")
            .field("parent_id", &self.parent_id)
            .finish()
    }
}

impl<Child: Model, const ASSOC: u8> PartialEq for HasMany<Child, ASSOC> {
    fn eq(&self, other: &Self) -> bool {
        self.parent_id == other.parent_id
    }
}

impl<Child: Model, const ASSOC: u8> Eq for HasMany<Child, ASSOC> {}
