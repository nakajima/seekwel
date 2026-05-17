//! Support types used by generated model params objects.

#[cfg(feature = "serde")]
use std::collections::BTreeMap;

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

#[cfg(feature = "serde")]
impl<T> Param<T>
where
    T: FromParamValue,
{
    /// Converts a raw param value into a provided typed params field.
    pub fn from_param_value(value: ParamValue) -> Result<Self, Error> {
        T::from_param_value(value).map(Self::provided)
    }
}

/// Raw form parameter value used when deserializing generated params.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// No value.
    Null,
    /// A scalar form value.
    Scalar(String),
    /// A list value built from repeated `[]` params or nested sequences.
    List(Vec<ParamValue>),
    /// A map value built from bracketed params or nested maps.
    Map(BTreeMap<String, ParamValue>),
}

#[cfg(feature = "serde")]
impl ParamValue {
    /// Creates a scalar param value.
    pub fn new(value: impl Into<String>) -> Self {
        Self::Scalar(value.into())
    }

    /// Creates an empty params map.
    pub fn map() -> Self {
        Self::Map(BTreeMap::new())
    }

    /// Returns the scalar value, if this is a scalar.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Scalar(raw) => Some(raw),
            Self::Null | Self::List(_) | Self::Map(_) => None,
        }
    }

    /// Consumes this param value and returns the scalar value, if this is a scalar.
    pub fn into_string(self) -> Option<String> {
        match self {
            Self::Scalar(raw) => Some(raw),
            Self::Null | Self::List(_) | Self::Map(_) => None,
        }
    }

    /// Inserts a flat form entry, parsing bracketed keys into maps and lists.
    pub fn insert_entry(&mut self, key: &str, value: Self) -> Result<(), Error> {
        let segments = param_key_segments(key);
        self.insert_path(&segments, value);
        Ok(())
    }

    /// Removes and returns a value at a nested path.
    pub fn take_path(&mut self, path: &[&str]) -> Option<Self> {
        let Self::Map(map) = self else {
            return None;
        };

        match path {
            [] => None,
            [key] => map.remove(*key),
            [key, rest @ ..] => map.get_mut(*key)?.take_path(rest),
        }
    }

    fn insert_path(&mut self, segments: &[&str], value: Self) {
        match segments {
            [] => *self = value,
            [segment, rest @ ..] if segment.is_empty() => {
                let list = self.as_list_mut();
                if rest.is_empty() {
                    list.push(value);
                } else if list.last().is_some_and(|last| last.can_insert_path(rest)) {
                    list.last_mut()
                        .expect("checked that the list has a last value")
                        .insert_path(rest, value);
                } else {
                    let mut child = Self::container_for(rest.first().copied());
                    child.insert_path(rest, value);
                    list.push(child);
                }
            }
            [segment] => {
                self.as_map_mut().insert((*segment).to_string(), value);
            }
            [segment, rest @ ..] => {
                self.as_map_mut()
                    .entry((*segment).to_string())
                    .or_insert_with(|| Self::container_for(rest.first().copied()))
                    .insert_path(rest, value);
            }
        }
    }

    fn can_insert_path(&self, segments: &[&str]) -> bool {
        match segments {
            [] => false,
            [segment, ..] if segment.is_empty() => matches!(self, Self::List(_)),
            [segment] => match self {
                Self::Map(map) => !map.contains_key(*segment),
                Self::Null | Self::Scalar(_) | Self::List(_) => false,
            },
            [segment, rest @ ..] => match self {
                Self::Map(map) => map
                    .get(*segment)
                    .is_none_or(|child| child.can_insert_path(rest)),
                Self::Null | Self::Scalar(_) | Self::List(_) => false,
            },
        }
    }

    fn as_list_mut(&mut self) -> &mut Vec<Self> {
        if !matches!(self, Self::List(_)) {
            *self = Self::List(Vec::new());
        }
        match self {
            Self::List(list) => list,
            Self::Null | Self::Scalar(_) | Self::Map(_) => unreachable!(),
        }
    }

    fn as_map_mut(&mut self) -> &mut BTreeMap<String, Self> {
        if !matches!(self, Self::Map(_)) {
            *self = Self::Map(BTreeMap::new());
        }
        match self {
            Self::Map(map) => map,
            Self::Null | Self::Scalar(_) | Self::List(_) => unreachable!(),
        }
    }

    fn container_for(next_segment: Option<&str>) -> Self {
        if matches!(next_segment, Some("")) {
            Self::List(Vec::new())
        } else {
            Self::Map(BTreeMap::new())
        }
    }

    fn into_scalar(self, expected: &'static str) -> Result<String, serde::de::value::Error> {
        match self {
            Self::Scalar(raw) => Ok(raw),
            Self::Null => Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Unit,
                &expected,
            )),
            Self::List(_) => Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Seq,
                &expected,
            )),
            Self::Map(_) => Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Map,
                &expected,
            )),
        }
    }
}

#[cfg(feature = "serde")]
fn param_key_segments(key: &str) -> Vec<&str> {
    let Some(first_bracket) = key.find('[') else {
        return vec![key];
    };
    let root = &key[..first_bracket];
    if root.is_empty() {
        return vec![key];
    }

    let mut segments = vec![root];
    let mut rest = &key[first_bracket..];
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return vec![key];
        }
        let Some(end) = rest.find(']') else {
            return vec![key];
        };
        segments.push(&rest[1..end]);
        rest = &rest[end + 1..];
    }

    segments
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

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ParamValue::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <ParamValue as serde::Deserialize>::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element::<ParamValue>()? {
            values.push(value);
        }
        Ok(ParamValue::List(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut values = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            values.insert(key, map.next_value::<ParamValue>()?);
        }
        Ok(ParamValue::Map(values))
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
struct ParamValueSeqAccess {
    values: std::vec::IntoIter<ParamValue>,
}

#[cfg(feature = "serde")]
impl<'de> serde::de::SeqAccess<'de> for ParamValueSeqAccess {
    type Error = serde::de::value::Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        self.values
            .next()
            .map(|value| seed.deserialize(value))
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.values.len())
    }
}

#[cfg(feature = "serde")]
struct ParamValueMapAccess {
    values: std::collections::btree_map::IntoIter<String, ParamValue>,
    next_value: Option<ParamValue>,
}

#[cfg(feature = "serde")]
impl<'de> serde::de::MapAccess<'de> for ParamValueMapAccess {
    type Error = serde::de::value::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.values.next() {
            Some((key, value)) => {
                self.next_value = Some(value);
                seed.deserialize(serde::de::IntoDeserializer::into_deserializer(key))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        let value = self
            .next_value
            .take()
            .ok_or_else(|| serde::de::Error::custom("missing map value"))?;
        seed.deserialize(value)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.values.len())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::de::IntoDeserializer<'de, serde::de::value::Error> for ParamValue {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserializer<'de> for ParamValue {
    type Error = serde::de::value::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Null => visitor.visit_unit(),
            Self::Scalar(raw) => visitor.visit_string(raw),
            Self::List(values) => visitor.visit_seq(ParamValueSeqAccess {
                values: values.into_iter(),
            }),
            Self::Map(values) => visitor.visit_map(ParamValueMapAccess {
                values: values.into_iter(),
                next_value: None,
            }),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let raw = self.into_scalar("one of 1, true, on, yes, 0, false, off, or no")?;
        let value = raw.trim();

        if ["1", "true", "on", "yes"]
            .iter()
            .any(|truthy| value.eq_ignore_ascii_case(truthy))
        {
            visitor.visit_bool(true)
        } else if ["0", "false", "off", "no"]
            .iter()
            .any(|falsey| value.eq_ignore_ascii_case(falsey))
        {
            visitor.visit_bool(false)
        } else {
            Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&raw),
                &"one of 1, true, on, yes, 0, false, off, or no",
            ))
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i8(parse_scalar(self, "an i8")?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i16(parse_scalar(self, "an i16")?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i32(parse_scalar(self, "an i32")?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i64(parse_scalar(self, "an i64")?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u8(parse_scalar(self, "a u8")?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u16(parse_scalar(self, "a u16")?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u32(parse_scalar(self, "a u32")?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u64(parse_scalar(self, "a u64")?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_f32(parse_scalar(self, "an f32")?)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_f64(parse_scalar(self, "an f64")?)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let raw = self.into_scalar("a single character")?;
        let mut chars = raw.chars();
        match (chars.next(), chars.next()) {
            (Some(value), None) => visitor.visit_char(value),
            _ => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&raw),
                &"a single character",
            )),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.into_scalar("a string")?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.into_scalar("a string")?)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.into_scalar("bytes")?.into_bytes())
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.into_scalar("bytes")?.into_bytes())
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Null => visitor.visit_none(),
            value => visitor.visit_some(value),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self {
            Self::Null => visitor.visit_unit(),
            Self::Scalar(raw) if raw.is_empty() => visitor.visit_unit(),
            Self::Scalar(raw) => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&raw),
                &"an empty string or null",
            )),
            Self::List(_) => Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Seq,
                &"an empty string or null",
            )),
            Self::Map(_) => Err(serde::de::Error::invalid_type(
                serde::de::Unexpected::Map,
                &"an empty string or null",
            )),
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

        visitor.visit_enum(self.into_scalar("an enum variant")?.into_deserializer())
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_string(self.into_scalar("an identifier")?)
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
fn parse_scalar<T>(value: ParamValue, expected: &'static str) -> Result<T, serde::de::value::Error>
where
    T: std::str::FromStr,
{
    let raw = value.into_scalar(expected)?;
    raw.parse()
        .map_err(|_| serde::de::Error::invalid_value(serde::de::Unexpected::Str(&raw), &expected))
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
        Self::from_param_value(value).map_err(serde::de::Error::custom)
    }
}
