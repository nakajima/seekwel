//! Support types used by generated model params objects.

use crate::model::Model;

/// A form/input params object generated for a model.
pub trait Params: Sized {
    /// The model this params object applies to.
    type Model: Model;
    /// The filtered params type returned by [`Params::allow`].
    type Allowed;

    /// Keeps only the listed columns available for model assignment.
    fn allow<I>(self, columns: I) -> Self::Allowed
    where
        I: IntoIterator<Item = <Self::Model as Model>::Column>;

    /// Keeps every column generated for this params object available for model assignment.
    fn allow_all(self) -> Self::Allowed;
}

/// Tracks whether a params field was provided.
#[derive(Debug, Clone)]
pub struct Param<T> {
    value: Option<T>,
}

impl<T> Default for Param<T> {
    fn default() -> Self {
        Self { value: None }
    }
}

impl<T> Param<T> {
    /// Creates a missing params field.
    pub fn missing() -> Self {
        Self::default()
    }

    /// Creates a provided params field.
    pub fn provided(value: T) -> Self {
        Self { value: Some(value) }
    }

    /// Returns whether this params field was provided.
    pub fn is_provided(&self) -> bool {
        self.value.is_some()
    }

    /// Returns the provided value by reference, if any.
    pub fn as_ref(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Consumes this field and returns the provided value, if any.
    pub fn into_value(self) -> Option<T> {
        self.value
    }
}

#[cfg(feature = "serde")]
impl<'de, T> serde::Deserialize<'de> for Param<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::provided)
    }
}
