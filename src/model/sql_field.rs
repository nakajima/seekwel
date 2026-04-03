use rusqlite::types::{Value, ValueRef};

use super::ColumnDef;

/// Describes how a Rust type is stored in SQLite.
///
/// Built-in implementations are provided for common scalar types, `String`,
/// `rusqlite::types::Value`, and `Option<T>`.
///
/// # Example
///
/// ```rust
/// use rusqlite::types::Value;
/// use seekwel::{Comparison, SqlField, connection::Connection, prelude::*};
///
/// #[derive(Debug, Clone, PartialEq, Eq)]
/// struct Email(String);
///
/// impl SqlField for Email {
///     const SQL_TYPE: &'static str = "TEXT";
///
///     fn to_sql_value(&self) -> Value {
///         Value::Text(self.0.clone())
///     }
///
///     fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
///         Ok(Self(row.get(index)?))
///     }
/// }
///
/// #[seekwel::model]
/// struct Contact {
///     id: u64,
///     email: Email,
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// Connection::memory()?;
/// Contact::create_table()?;
/// Contact::builder()
///     .email(Email("pat@example.com".to_string()))
///     .create()?;
///
/// let found = Contact::q(
///     ContactColumns::Email,
///     Comparison::Eq(Email("pat@example.com".to_string())),
/// )
/// .first()?;
/// assert!(found.is_some());
/// # Ok(())
/// # }
/// ```
pub trait SqlField: Sized {
    /// The SQLite type used when creating columns of this field type.
    const SQL_TYPE: &'static str;
    /// Whether values of this type may be stored as `NULL`.
    const NULLABLE: bool = false;

    /// Converts the Rust value into a SQLite value for inserts and updates.
    fn to_sql_value(&self) -> Value;
    /// Reads the value from a SQLite row at `index`.
    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self>;

    /// Converts the value into a comparison operand.
    ///
    /// The default implementation uses [`Self::to_sql_value`]. Types like
    /// `Option<T>` override this to represent SQL `NULL`.
    fn into_sql_comparison_value(self) -> Option<Value> {
        Some(self.to_sql_value())
    }
}

#[doc(hidden)]
pub const fn column<T: SqlField>(name: &'static str) -> ColumnDef {
    ColumnDef {
        name,
        sql_type: T::SQL_TYPE,
        nullable: T::NULLABLE,
    }
}

macro_rules! integer_sql_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl SqlField for $ty {
                const SQL_TYPE: &'static str = "INTEGER";

                fn to_sql_value(&self) -> Value {
                    Value::Integer(*self as i64)
                }

                fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
                    row.get(index)
                }
            }
        )*
    };
}

macro_rules! float_sql_field {
    ($($ty:ty),* $(,)?) => {
        $(
            impl SqlField for $ty {
                const SQL_TYPE: &'static str = "REAL";

                fn to_sql_value(&self) -> Value {
                    Value::Real(*self as f64)
                }

                fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
                    row.get(index)
                }
            }
        )*
    };
}

integer_sql_field!(u8, u16, u32, i8, i16, i32, i64, bool);
float_sql_field!(f32, f64);

impl SqlField for u64 {
    const SQL_TYPE: &'static str = "INTEGER";

    fn to_sql_value(&self) -> Value {
        Value::Integer(*self as i64)
    }

    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
        Ok(row.get::<_, i64>(index)? as u64)
    }
}

impl SqlField for String {
    const SQL_TYPE: &'static str = "TEXT";

    fn to_sql_value(&self) -> Value {
        Value::Text(self.clone())
    }

    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
        row.get(index)
    }
}

impl SqlField for Value {
    const SQL_TYPE: &'static str = "BLOB";

    fn to_sql_value(&self) -> Value {
        self.clone()
    }

    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
        row.get(index)
    }
}

impl<T> SqlField for Option<T>
where
    T: SqlField,
{
    const SQL_TYPE: &'static str = T::SQL_TYPE;
    const NULLABLE: bool = true;

    fn to_sql_value(&self) -> Value {
        match self {
            Some(value) => value.to_sql_value(),
            None => Value::Null,
        }
    }

    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
        match row.get_ref(index)? {
            ValueRef::Null => Ok(None),
            _ => T::from_sql_row(row, index).map(Some),
        }
    }

    fn into_sql_comparison_value(self) -> Option<Value> {
        self.map(|value| value.to_sql_value())
    }
}
