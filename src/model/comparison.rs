use rusqlite::types::Value;

use crate::error::Error;

use super::SqlField;

/// A typed comparison used in query predicates.
///
/// `Eq(None::<T>)` becomes `IS NULL`, and `Ne(None::<T>)` becomes
/// `IS NOT NULL`.
#[derive(Debug, Clone)]
pub enum Comparison<T> {
    /// Equal to a value.
    Eq(T),
    /// Not equal to a value.
    Ne(T),
    /// Greater than a value.
    Gt(T),
    /// Greater than or equal to a value.
    Gte(T),
    /// Less than a value.
    Lt(T),
    /// Less than or equal to a value.
    Lte(T),
}

/// A value that can appear on the right-hand side of a [`Comparison`].
pub trait ComparisonOperand {
    /// Converts the value into an owned SQLite comparison value.
    ///
    /// Returning `None` represents SQL `NULL`.
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

impl<T> ComparisonOperand for T
where
    T: SqlField,
{
    fn into_sql_value(self) -> Option<Value> {
        self.into_sql_comparison_value()
    }
}

impl ComparisonOperand for &str {
    fn into_sql_value(self) -> Option<Value> {
        Some(Value::Text(self.to_string()))
    }
}
