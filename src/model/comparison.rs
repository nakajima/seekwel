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
    value: ComparisonValue,
}

#[derive(Debug, Clone)]
enum ComparisonValue {
    Single(Option<Value>),
    Many(Vec<Value>),
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
    In,
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

    #[allow(non_snake_case)]
    /// Creates an `IN (...)` comparison.
    pub fn In<I, T>(values: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: ComparisonOperand,
    {
        Self {
            operator: ComparisonOperator::In,
            value: ComparisonValue::Many(
                values
                    .into_iter()
                    .filter_map(ComparisonOperand::into_sql_value)
                    .collect(),
            ),
        }
    }

    #[allow(non_upper_case_globals)]
    /// Tests whether a column is `NULL`.
    pub const IsNull: Self = Self {
        operator: ComparisonOperator::IsNull,
        value: ComparisonValue::Single(None),
    };

    #[allow(non_upper_case_globals)]
    /// Tests whether a column is not `NULL`.
    pub const IsNotNull: Self = Self {
        operator: ComparisonOperator::IsNotNull,
        value: ComparisonValue::Single(None),
    };

    fn with_value<T>(operator: ComparisonOperator, value: T) -> Self
    where
        T: ComparisonOperand,
    {
        Self {
            operator,
            value: ComparisonValue::Single(value.into_sql_value()),
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
            (ComparisonOperator::Eq, ComparisonValue::Single(Some(value))) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} = {placeholder}"))
            }
            (ComparisonOperator::Eq, ComparisonValue::Single(None))
            | (ComparisonOperator::IsNull, ComparisonValue::Single(_)) => {
                Ok(format!("{column} IS NULL"))
            }
            (ComparisonOperator::Ne, ComparisonValue::Single(Some(value))) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} != {placeholder}"))
            }
            (ComparisonOperator::Ne, ComparisonValue::Single(None))
            | (ComparisonOperator::IsNotNull, ComparisonValue::Single(_)) => {
                Ok(format!("{column} IS NOT NULL"))
            }
            (ComparisonOperator::Gt, ComparisonValue::Single(Some(value))) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} > {placeholder}"))
            }
            (ComparisonOperator::Gte, ComparisonValue::Single(Some(value))) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} >= {placeholder}"))
            }
            (ComparisonOperator::Lt, ComparisonValue::Single(Some(value))) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} < {placeholder}"))
            }
            (ComparisonOperator::Lte, ComparisonValue::Single(Some(value))) => {
                let placeholder = push_placeholder(params, value);
                Ok(format!("{column} <= {placeholder}"))
            }
            (ComparisonOperator::In, ComparisonValue::Many(values)) => {
                if values.is_empty() {
                    return Ok("0 = 1".to_string());
                }
                let placeholders = values
                    .into_iter()
                    .map(|value| push_placeholder(params, value))
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(format!("{column} IN ({placeholders})"))
            }
            (operator, ComparisonValue::Single(None)) => Err(Error::InvalidQuery(format!(
                "{} comparisons do not support NULL",
                operator.as_str()
            ))),
            (operator, ComparisonValue::Single(Some(_))) => Err(Error::InvalidQuery(format!(
                "{} comparisons do not support this value shape",
                operator.as_str()
            ))),
            (operator, ComparisonValue::Many(_)) => Err(Error::InvalidQuery(format!(
                "{} comparisons do not support list values here",
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
            ComparisonOperator::In => "In",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_clause_with_all_none_operands_matches_no_rows() {
        let values: Vec<Option<i64>> = vec![None, None];
        let mut params = Vec::new();
        let clause = Comparison::In(values)
            .into_prepared()
            .into_clause("col", &mut params)
            .expect("IN with all-None operands should render a clause");

        assert_eq!(clause, "0 = 1");
        assert!(
            params.is_empty(),
            "no placeholders should be bound when every operand was filtered out",
        );
    }
}
