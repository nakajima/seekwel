//! Support types used by generated model builders.

use crate::error::Error;

/// Tracks whether a required builder field has been provided.
#[derive(Debug, Clone)]
pub struct Required<T>(Option<T>);

impl<T> Default for Required<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Required<T> {
    /// Stores the field value.
    pub fn set(&mut self, value: impl Into<T>) {
        self.0 = Some(value.into());
    }

    /// Finishes the field or returns [`Error::MissingField`] if it was never set.
    pub fn finish(self, field: &'static str) -> Result<T, Error> {
        self.0.ok_or_else(|| Error::MissingField(field.to_string()))
    }
}

/// Tracks whether an optional builder field has been explicitly set.
#[derive(Debug, Clone)]
pub struct Optional<T>(Option<Option<T>>);

impl<T> Default for Optional<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Optional<T> {
    /// Stores the field value, including an explicit `None`.
    pub fn set(&mut self, value: Option<T>) {
        self.0 = Some(value);
    }

    /// Finishes the field, defaulting to `None` if it was never set.
    pub fn finish(self) -> Option<T> {
        self.0.unwrap_or(None)
    }
}
