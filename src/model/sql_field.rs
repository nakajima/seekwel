use rusqlite::types::{Value, ValueRef};

use super::ColumnDef;

pub trait SqlField: Sized {
    const SQL_TYPE: &'static str;
    const NULLABLE: bool = false;

    fn to_sql_value(&self) -> Value;
    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self>;

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
