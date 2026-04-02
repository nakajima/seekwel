use rusqlite::types::Value;

use crate::error::Error;

#[derive(Debug, Clone)]
pub enum Comparison<T> {
    Eq(T),
    Ne(T),
    Gt(T),
    Gte(T),
    Lt(T),
    Lte(T),
}

pub trait ComparisonOperand {
    fn into_sql_value(self) -> Option<Value>;
}

#[derive(Debug, Clone, Copy)]
enum ComparisonOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedComparison {
    operator: ComparisonOperator,
    value: Option<Value>,
}

impl<T> Comparison<T>
where
    T: ComparisonOperand,
{
    pub(super) fn into_prepared(self) -> PreparedComparison {
        match self {
            Comparison::Eq(value) => PreparedComparison {
                operator: ComparisonOperator::Eq,
                value: value.into_sql_value(),
            },
            Comparison::Ne(value) => PreparedComparison {
                operator: ComparisonOperator::Ne,
                value: value.into_sql_value(),
            },
            Comparison::Gt(value) => PreparedComparison {
                operator: ComparisonOperator::Gt,
                value: value.into_sql_value(),
            },
            Comparison::Gte(value) => PreparedComparison {
                operator: ComparisonOperator::Gte,
                value: value.into_sql_value(),
            },
            Comparison::Lt(value) => PreparedComparison {
                operator: ComparisonOperator::Lt,
                value: value.into_sql_value(),
            },
            Comparison::Lte(value) => PreparedComparison {
                operator: ComparisonOperator::Lte,
                value: value.into_sql_value(),
            },
        }
    }
}

impl PreparedComparison {
    pub(super) fn into_clause(
        self,
        column: &str,
        params: &mut Vec<Value>,
    ) -> Result<String, Error> {
        match (self.operator, self.value) {
            (ComparisonOperator::Eq, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} = {placeholder}"))
            }
            (ComparisonOperator::Eq, None) => Ok(format!("{column} IS NULL")),
            (ComparisonOperator::Ne, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} != {placeholder}"))
            }
            (ComparisonOperator::Ne, None) => Ok(format!("{column} IS NOT NULL")),
            (ComparisonOperator::Gt, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} > {placeholder}"))
            }
            (ComparisonOperator::Gte, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} >= {placeholder}"))
            }
            (ComparisonOperator::Lt, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} < {placeholder}"))
            }
            (ComparisonOperator::Lte, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} <= {placeholder}"))
            }
            (operator, None) => Err(Error::InvalidQuery(format!(
                "{} comparisons do not support NULL",
                operator.as_str()
            ))),
        }
    }
}

impl ComparisonOperator {
    fn as_str(self) -> &'static str {
        match self {
            ComparisonOperator::Eq => "Eq",
            ComparisonOperator::Ne => "Ne",
            ComparisonOperator::Gt => "Gt",
            ComparisonOperator::Gte => "Gte",
            ComparisonOperator::Lt => "Lt",
            ComparisonOperator::Lte => "Lte",
        }
    }
}

fn push_placeholder(params: &mut Vec<Value>, value: Value) -> String {
    let index = params.len() + 1;
    params.push(value);
    format!("?{index}")
}

macro_rules! integer_operand {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ComparisonOperand for $ty {
                fn into_sql_value(self) -> Option<Value> {
                    Some(Value::Integer(self as i64))
                }
            }

            impl ComparisonOperand for &$ty {
                fn into_sql_value(self) -> Option<Value> {
                    Some(Value::Integer(*self as i64))
                }
            }
        )*
    };
}

macro_rules! float_operand {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ComparisonOperand for $ty {
                fn into_sql_value(self) -> Option<Value> {
                    Some(Value::Real(self as f64))
                }
            }

            impl ComparisonOperand for &$ty {
                fn into_sql_value(self) -> Option<Value> {
                    Some(Value::Real(*self as f64))
                }
            }
        )*
    };
}

integer_operand!(u8, u16, u32, u64, i8, i16, i32, i64);
float_operand!(f32, f64);

impl ComparisonOperand for bool {
    fn into_sql_value(self) -> Option<Value> {
        Some(Value::Integer(self as i64))
    }
}

impl ComparisonOperand for &bool {
    fn into_sql_value(self) -> Option<Value> {
        Some(Value::Integer(*self as i64))
    }
}

impl ComparisonOperand for String {
    fn into_sql_value(self) -> Option<Value> {
        Some(Value::Text(self))
    }
}

impl ComparisonOperand for &String {
    fn into_sql_value(self) -> Option<Value> {
        Some(Value::Text(self.clone()))
    }
}

impl ComparisonOperand for &str {
    fn into_sql_value(self) -> Option<Value> {
        Some(Value::Text(self.to_string()))
    }
}

impl ComparisonOperand for Value {
    fn into_sql_value(self) -> Option<Value> {
        Some(self)
    }
}

impl ComparisonOperand for &Value {
    fn into_sql_value(self) -> Option<Value> {
        Some(self.clone())
    }
}

impl<T> ComparisonOperand for Option<T>
where
    T: ComparisonOperand,
{
    fn into_sql_value(self) -> Option<Value> {
        match self {
            Some(value) => value.into_sql_value(),
            None => None,
        }
    }
}
