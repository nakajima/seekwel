use rusqlite::types::Value;

use crate::error::Error;

use super::SqlField;

/// A comparison used in query predicates.
///
/// `Comparison::Eq(None::<T>)` becomes `IS NULL`, and `Comparison::Ne(None::<T>)`
/// becomes `IS NOT NULL` for backward compatibility. Prefer
/// [`Comparison::IsNull`] and [`Comparison::IsNotNull`] for new code.
#[derive(Debug, Clone)]
pub struct Comparison {
    operator: ComparisonOperator,
    value: Option<Value>,
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
    IsNull,
    IsNotNull,
}

pub(super) type PreparedComparison = Comparison;

impl Comparison {
    #[allow(non_snake_case)]
    /// Creates an equality comparison.
    pub fn Eq<T>(value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self::with_value(ComparisonOperator::Eq, value)
    }

    #[allow(non_snake_case)]
    /// Creates an inequality comparison.
    pub fn Ne<T>(value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self::with_value(ComparisonOperator::Ne, value)
    }

    #[allow(non_snake_case)]
    /// Creates a greater-than comparison.
    pub fn Gt<T>(value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self::with_value(ComparisonOperator::Gt, value)
    }

    #[allow(non_snake_case)]
    /// Creates a greater-than-or-equal comparison.
    pub fn Gte<T>(value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self::with_value(ComparisonOperator::Gte, value)
    }

    #[allow(non_snake_case)]
    /// Creates a less-than comparison.
    pub fn Lt<T>(value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self::with_value(ComparisonOperator::Lt, value)
    }

    #[allow(non_snake_case)]
    /// Creates a less-than-or-equal comparison.
    pub fn Lte<T>(value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self::with_value(ComparisonOperator::Lte, value)
    }

    #[allow(non_upper_case_globals)]
    /// Tests whether a column is `NULL`.
    pub const IsNull: Self = Self {
        operator: ComparisonOperator::IsNull,
        value: None,
    };

    #[allow(non_upper_case_globals)]
    /// Tests whether a column is not `NULL`.
    pub const IsNotNull: Self = Self {
        operator: ComparisonOperator::IsNotNull,
        value: None,
    };

    fn with_value<T>(operator: ComparisonOperator, value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self {
            operator,
            value: value.into_sql_value(),
        }
    }

    pub(super) fn into_prepared(self) -> PreparedComparison {
        self
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
            (ComparisonOperator::Eq, None)
            | (ComparisonOperator::IsNull, None)
            | (ComparisonOperator::IsNull, Some(_)) => Ok(format!("{column} IS NULL")),
            (ComparisonOperator::Ne, Some(value)) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} != {placeholder}"))
            }
            (ComparisonOperator::Ne, None)
            | (ComparisonOperator::IsNotNull, None)
            | (ComparisonOperator::IsNotNull, Some(_)) => Ok(format!("{column} IS NOT NULL")),
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
            ComparisonOperator::IsNull => "IsNull",
            ComparisonOperator::IsNotNull => "IsNotNull",
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
