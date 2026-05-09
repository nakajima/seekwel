//! Validation support for model saves.

use std::fmt;

use crate::error::Error;

use super::{Column, Model};

/// A single validation error attached to either a column or the model as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError<C> {
    column: Option<C>,
    message: String,
}

impl<C> ValidationError<C> {
    /// Creates a validation error for a column.
    pub fn on(column: C, message: impl Into<String>) -> Self {
        Self {
            column: Some(column),
            message: message.into(),
        }
    }

    /// Creates a validation error for the model as a whole.
    pub fn base(message: impl Into<String>) -> Self {
        Self {
            column: None,
            message: message.into(),
        }
    }

    /// Returns the column this error belongs to, if any.
    pub fn column(&self) -> Option<C>
    where
        C: Copy,
    {
        self.column
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Rails-style validation errors for a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errors<C> {
    errors: Vec<ValidationError<C>>,
}

impl<C> Default for Errors<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> Errors<C> {
    /// Creates an empty error collection.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Adds an error to a column.
    pub fn add(&mut self, column: C, message: impl Into<String>) {
        self.errors.push(ValidationError::on(column, message));
    }

    /// Adds an error to the model as a whole.
    pub fn add_base(&mut self, message: impl Into<String>) {
        self.errors.push(ValidationError::base(message));
    }

    /// Returns all collected errors.
    pub fn all(&self) -> &[ValidationError<C>] {
        &self.errors
    }

    /// Returns whether no errors were collected.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns the number of collected errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Returns messages attached to the given column.
    pub fn on(&self, column: C) -> Vec<&str>
    where
        C: Copy + PartialEq,
    {
        self.errors
            .iter()
            .filter_map(|error| match error.column {
                Some(error_column) if error_column == column => Some(error.message.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Returns messages attached to the model as a whole.
    pub fn base(&self) -> Vec<&str> {
        self.errors
            .iter()
            .filter_map(|error| error.column.is_none().then_some(error.message.as_str()))
            .collect()
    }

    /// Returns Rails-style full messages like `name can't be blank`.
    pub fn full_messages(&self) -> Vec<String>
    where
        C: Column,
    {
        self.errors
            .iter()
            .map(|error| match error.column {
                Some(column) => format!("{} {}", column.as_str(), error.message),
                None => error.message.clone(),
            })
            .collect()
    }
}

/// Typestate for a model that failed validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid<S, C> {
    previous: S,
    errors: Errors<C>,
}

impl<S, C> Invalid<S, C> {
    /// Creates an invalid typestate from the previous state and collected errors.
    pub fn new(previous: S, errors: Errors<C>) -> Self {
        Self { previous, errors }
    }

    /// Returns the state this model had before validation failed.
    pub fn previous(&self) -> &S {
        &self.previous
    }

    /// Returns validation errors.
    pub fn errors(&self) -> &Errors<C> {
        &self.errors
    }

    /// Consumes the typestate and returns validation errors.
    pub fn into_errors(self) -> Errors<C> {
        self.errors
    }
}

/// Behavior exposed by generated invalid model values.
pub trait InvalidModel: Model {
    /// The model state before validation failed.
    type PreviousState;

    /// Returns validation errors.
    fn errors(&self) -> &Errors<Self::Column>;
}

/// A validation hook used by generated model implementations.
pub trait Validator<M: Model> {
    /// Adds validation errors for `model`.
    fn validate(model: &M, errors: &mut Errors<M::Column>);
}

/// The default validation hook. It accepts every model value.
pub struct NoValidation;

impl<M: Model> Validator<M> for NoValidation {
    fn validate(_model: &M, _errors: &mut Errors<M::Column>) {}
}

/// Error returned by save operations.
pub enum SaveError<M> {
    /// The model failed validation. The invalid model carries its errors.
    Invalid(M),
    /// A non-validation error occurred while saving.
    Error(Error),
}

impl<M> SaveError<M> {
    /// Returns whether this is a validation error.
    pub fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }

    /// Returns the invalid model by reference, if this is a validation error.
    pub fn invalid(&self) -> Option<&M> {
        match self {
            Self::Invalid(model) => Some(model),
            Self::Error(_) => None,
        }
    }

    /// Consumes this error and returns the invalid model, if this is a validation error.
    pub fn into_invalid(self) -> Option<M> {
        match self {
            Self::Invalid(model) => Some(model),
            Self::Error(_) => None,
        }
    }
}

impl<M> From<Error> for SaveError<M> {
    fn from(error: Error) -> Self {
        Self::Error(error)
    }
}

impl<M> From<SaveError<M>> for Error {
    fn from(error: SaveError<M>) -> Self {
        match error {
            SaveError::Invalid(_) => Error::InvalidModel("validation failed".to_string()),
            SaveError::Error(error) => error,
        }
    }
}

impl<M> fmt::Debug for SaveError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(_) => f.write_str("Invalid(..)"),
            Self::Error(error) => f.debug_tuple("Error").field(error).finish(),
        }
    }
}

impl<M> fmt::Display for SaveError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(_) => f.write_str("validation failed"),
            Self::Error(error) => write!(f, "{error}"),
        }
    }
}

impl<M> std::error::Error for SaveError<M> {}
