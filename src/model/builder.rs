use crate::error::Error;

#[derive(Debug, Clone)]
pub struct Required<T>(Option<T>);

impl<T> Default for Required<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Required<T> {
    pub fn set(&mut self, value: impl Into<T>) {
        self.0 = Some(value.into());
    }

    pub fn finish(self, field: &'static str) -> Result<T, Error> {
        self.0.ok_or_else(|| Error::MissingField(field.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct Optional<T>(Option<Option<T>>);

impl<T> Default for Optional<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T> Optional<T> {
    pub fn set(&mut self, value: Option<T>) {
        self.0 = Some(value);
    }

    pub fn finish(self) -> Option<T> {
        self.0.unwrap_or(None)
    }
}
