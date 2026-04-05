use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;

use rusqlite::types::Value;

use crate::error::Error;

use super::{PersistedModel, SqlField};

/// A typed foreign-key reference to another persisted model.
///
/// `BelongsTo<T>` stores the related model's primary key and can lazily load
/// the parent record on demand.
///
/// # Example
///
/// ```rust
/// use seekwel::{BelongsTo, connection::Connection, prelude::*};
///
/// #[seekwel::model]
/// struct Person {
///     id: u64,
///     name: String,
/// }
///
/// #[seekwel::model]
/// struct Pet {
///     id: u64,
///     name: String,
///     owner: BelongsTo<Person>,
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// Connection::memory()?;
/// Person::create_table()?;
/// Pet::create_table()?;
///
/// let pat = Person::builder().name("Pat").create()?;
/// let pet = Pet::builder().name("Fido").owner(pat.id).create()?;
///
/// assert_eq!(pet.owner()?.name, "Pat");
/// # Ok(())
/// # }
/// ```
pub struct BelongsTo<T> {
    id: u64,
    cached: RefCell<Option<T>>,
    __seekwel_target: PhantomData<T>,
}

impl<T> BelongsTo<T> {
    /// Creates a relation wrapper from a persisted primary key.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            cached: RefCell::new(None),
            __seekwel_target: PhantomData,
        }
    }

    /// Returns the related model's primary key.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Clears any cached parent model for this relation.
    pub fn clear_cache(&self) {
        self.cached.borrow_mut().take();
    }

    /// Loads the related persisted model, caching it on first access.
    pub fn load(&self) -> Result<T, Error>
    where
        T: PersistedModel + Clone,
    {
        if let Some(cached) = self.cached.borrow().as_ref() {
            return Ok(cached.clone());
        }

        let model = T::find(self.id)?;
        *self.cached.borrow_mut() = Some(model.clone());
        Ok(model)
    }
}

impl<T> BelongsTo<T>
where
    T: PersistedModel,
{
    pub(crate) fn with_cached(model: T) -> Self {
        Self {
            id: model.id(),
            cached: RefCell::new(Some(model)),
            __seekwel_target: PhantomData,
        }
    }
}

impl<T> Clone for BelongsTo<T> {
    fn clone(&self) -> Self {
        Self::new(self.id)
    }
}

impl<T> fmt::Debug for BelongsTo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BelongsTo").field("id", &self.id).finish()
    }
}

impl<T> PartialEq for BelongsTo<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for BelongsTo<T> {}

impl<T> From<u64> for BelongsTo<T> {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

impl<M> From<M> for BelongsTo<M>
where
    M: PersistedModel,
{
    fn from(model: M) -> Self {
        Self::with_cached(model)
    }
}

impl<M> From<&M> for BelongsTo<M>
where
    M: PersistedModel,
{
    fn from(model: &M) -> Self {
        Self::new(model.id())
    }
}

impl<T> SqlField for BelongsTo<T> {
    const SQL_TYPE: &'static str = "INTEGER";

    fn to_sql_value(&self) -> Value {
        Value::Integer(self.id as i64)
    }

    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
        Ok(Self::new(row.get::<_, i64>(index)? as u64))
    }
}
