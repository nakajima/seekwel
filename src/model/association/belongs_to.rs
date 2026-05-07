use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;

use rusqlite::types::Value;

use crate::error::Error;
use crate::model::{PersistedModel, SqlField};

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
    /// Creates an association wrapper from a persisted primary key.
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

    /// Clears any cached parent model for this association.
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

impl<M> PartialEq<M> for BelongsTo<M>
where
    M: PersistedModel,
{
    fn eq(&self, other: &M) -> bool {
        self.id == other.id()
    }
}

impl<T> Eq for BelongsTo<T> {}

impl<T> From<u64> for BelongsTo<T> {
    fn from(id: u64) -> Self {
        Self::new(id)
    }
}

macro_rules! impl_belongs_to_from_unsigned {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<T> From<$ty> for BelongsTo<T> {
                fn from(id: $ty) -> Self {
                    Self::new(id as u64)
                }
            }
        )*
    };
}

macro_rules! impl_belongs_to_from_signed {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<T> From<$ty> for BelongsTo<T> {
                fn from(id: $ty) -> Self {
                    Self::new(u64::try_from(id).expect("BelongsTo ids must be non-negative"))
                }
            }
        )*
    };
}

impl_belongs_to_from_unsigned!(u32, u16, u8);
impl_belongs_to_from_signed!(i64, i32, i16, i8);

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
        let value = row.get::<_, i64>(index)?;
        let id = u64::try_from(value).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::other("BelongsTo ids must be non-negative")),
            )
        })?;
        Ok(Self::new(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Target;

    #[test]
    fn from_u32_round_trips_through_u64() {
        let association: BelongsTo<Target> = 42_u32.into();
        assert_eq!(association.id(), 42);
    }

    #[test]
    fn from_i64_round_trips_for_non_negative_values() {
        let association: BelongsTo<Target> = 42_i64.into();
        assert_eq!(association.id(), 42);
    }

    #[test]
    #[should_panic(expected = "BelongsTo ids must be non-negative")]
    fn from_i64_panics_on_negative_values() {
        let _: BelongsTo<Target> = (-1_i64).into();
    }
}
