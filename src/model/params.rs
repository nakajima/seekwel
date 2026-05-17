//! Support types used by generated model params objects.

use crate::error::Error;
use crate::model::{Model, NewModel, PersistedModel, SaveError};

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

/// Hooks that connect a persisted model to its generated params object.
pub trait ParamsModel: PersistedModel + Sized {
    /// The new-record type built by params assignment.
    type NewRecord: NewModel<Persisted = Self>;
    /// The generated params type for this model.
    type Params: Params<Model = Self>;

    /// Builds a new record from filtered params.
    fn build_from_params(
        params: <Self::Params as Params>::Allowed,
    ) -> Result<Self::NewRecord, Error>;

    /// Applies filtered params to this persisted record without saving it.
    fn apply_params(&mut self, params: <Self::Params as Params>::Allowed) -> Result<(), Error>;
}

/// Model-level params entrypoints exposed as associated functions and methods.
pub trait ParamsModelDsl: ParamsModel {
    /// Builds a new record from filtered params.
    fn new(params: <Self::Params as Params>::Allowed) -> Result<Self::NewRecord, Error> {
        Self::build_from_params(params)
    }

    /// Builds and inserts a persisted record from filtered params.
    fn create(
        params: <Self::Params as Params>::Allowed,
    ) -> Result<Self, SaveError<<Self::NewRecord as NewModel>::Invalid>> {
        <Self::NewRecord as NewModel>::save(
            Self::build_from_params(params).map_err(SaveError::Error)?,
        )
    }

    /// Applies filtered params and persists the updated record.
    fn update(
        &mut self,
        params: <Self::Params as Params>::Allowed,
    ) -> Result<(), SaveError<Self::Invalid>> {
        self.apply_params(params).map_err(SaveError::Error)?;
        <Self as PersistedModel>::save(self)
    }
}

impl<M> ParamsModelDsl for M where M: ParamsModel {}

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

/// Raw form parameter value used when deserializing generated params.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq)]
pub struct ParamValue {
    raw: String,
}

#[cfg(feature = "serde")]
impl ParamValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self { raw: value.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn into_string(self) -> String {
        self.raw
    }

    fn bool_value(&self) -> Option<bool> {
        let value = self.raw.trim();

        if ["1", "true", "on", "yes"]
            .iter()
            .any(|truthy| value.eq_ignore_ascii_case(truthy))
        {
            Some(true)
        } else if ["0", "false", "off", "no"]
            .iter()
            .any(|falsey| value.eq_ignore_ascii_case(falsey))
        {
            Some(false)
        } else {
            None
        }
    }

    fn parse<T>(&self, expected: &'static str) -> Result<T, serde::de::value::Error>
    where
        T: std::str::FromStr,
    {
        self.raw.parse().map_err(|_| {
            serde::de::Error::invalid_value(serde::de::Unexpected::Str(&self.raw), &expected)
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ParamValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(ParamValueVisitor)
    }
}

#[cfg(feature = "serde")]
struct ParamValueVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for ParamValueVisitor {
    type Value = ParamValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a form parameter value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::new(if value { "true" } else { "false" }))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::new(value.to_string()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::new(value.to_string()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::new(value.to_string()))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::new(value))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::new(value))
    }
}

/// Converts a raw form parameter into a typed model field value.
#[cfg(feature = "serde")]
pub trait FromParamValue: Sized {
    /// Converts a raw param value into `Self`.
    fn from_param_value(value: ParamValue) -> Result<Self, Error>;
}

#[cfg(feature = "serde")]
impl<T> FromParamValue for T
where
    T: serde::de::DeserializeOwned,
{
    fn from_param_value(value: ParamValue) -> Result<Self, Error> {
        T::deserialize(value).map_err(|error| Error::InvalidParams(error.to_string()))
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserializer<'de> for ParamValue {
    type Error = serde::de::value::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.raw)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self.bool_value() {
            Some(value) => visitor.visit_bool(value),
            None => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&self.raw),
                &"one of 1, true, on, yes, 0, false, off, or no",
            )),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i8(self.parse("an i8")?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i16(self.parse("an i16")?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i32(self.parse("an i32")?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i64(self.parse("an i64")?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u8(self.parse("a u8")?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u16(self.parse("a u16")?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u32(self.parse("a u32")?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u64(self.parse("a u64")?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_f32(self.parse("an f32")?)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_f64(self.parse("an f64")?)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let mut chars = self.raw.chars();
        match (chars.next(), chars.next()) {
            (Some(value), None) => visitor.visit_char(value),
            _ => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&self.raw),
                &"a single character",
            )),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.raw)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.raw)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.raw.into_bytes())
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.raw.into_bytes())
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        if self.raw.is_empty() {
            visitor.visit_unit()
        } else {
            Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&self.raw),
                &"an empty string",
            ))
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        use serde::de::IntoDeserializer;

        visitor.visit_enum(self.raw.into_deserializer())
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.raw)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    serde::forward_to_deserialize_any! {
        seq tuple tuple_struct map struct
    }
}

#[cfg(feature = "serde")]
impl<'de, T> serde::Deserialize<'de> for Param<T>
where
    T: FromParamValue,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <ParamValue as serde::Deserialize>::deserialize(deserializer)?;
        T::from_param_value(value)
            .map(Self::provided)
            .map_err(serde::de::Error::custom)
    }
}
