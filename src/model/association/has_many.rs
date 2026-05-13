use std::fmt;
use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard};

use crate::error::Error;
use crate::model::PersistedModel;

/// Internal child-side metadata shared by all [`HasMany`] associations for a model.
#[doc(hidden)]
pub trait HasManyChild: PersistedModel + Clone + Sized {
    type Builder;
}

/// Runtime handlers for a specific generated [`HasMany`] foreign key.
#[doc(hidden)]
pub struct HasManyHandlers<Child: HasManyChild, Parent = ()> {
    load_for_parent: fn(u64) -> Result<Vec<Child>, Error>,
    append_for_parent: fn(u64, <Child as HasManyChild>::Builder) -> Result<Child, Error>,
    __seekwel_parent: PhantomData<fn() -> Parent>,
}

impl<Child: HasManyChild, Parent> HasManyHandlers<Child, Parent> {
    #[doc(hidden)]
    pub fn new(
        load_for_parent: fn(u64) -> Result<Vec<Child>, Error>,
        append_for_parent: fn(u64, <Child as HasManyChild>::Builder) -> Result<Child, Error>,
    ) -> Self {
        Self {
            load_for_parent,
            append_for_parent,
            __seekwel_parent: PhantomData,
        }
    }

    fn erase_parent(self) -> HasManyHandlers<Child> {
        HasManyHandlers::new(self.load_for_parent, self.append_for_parent)
    }
}

impl<Child: HasManyChild> HasManyHandlers<Child> {
    fn missing() -> Self {
        Self::new(
            missing_load_for_parent::<Child>,
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

fn missing_load_for_parent<Child: HasManyChild>(_parent_id: u64) -> Result<Vec<Child>, Error> {
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
/// child rows, and can append new children through the generated child builder.
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

    /// Loads all child records for the bound parent, caching them on first access.
    pub fn load(&self) -> Result<Vec<Child>, Error> {
        {
            let cached = self.cached();
            if let Some(children) = cached.as_ref() {
                return Ok(children.clone());
            }
        }

        let children = (self.handlers.load_for_parent)(self.require_parent_id()?)?;
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
