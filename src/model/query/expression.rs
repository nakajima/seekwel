use rusqlite::types::Value;

use crate::error::Error;

use super::super::comparison::PreparedComparison;

#[derive(Debug, Clone)]
pub(super) enum QueryExpression {
    Predicate {
        column: &'static str,
        comparison: PreparedComparison,
    },
    And(Box<QueryExpression>, Box<QueryExpression>),
    Or(Box<QueryExpression>, Box<QueryExpression>),
}

impl QueryExpression {
    pub(super) fn into_clause(self, params: &mut Vec<Value>) -> Result<String, Error> {
        match self {
            QueryExpression::Predicate { column, comparison } => {
                comparison.into_clause(column, params)
            }
            QueryExpression::And(left, right) => {
                let left = left.into_clause(params)?;
                let right = right.into_clause(params)?;
                Ok(format!("({left}) AND ({right})"))
            }
            QueryExpression::Or(left, right) => {
                let left = left.into_clause(params)?;
                let right = right.into_clause(params)?;
                Ok(format!("({left}) OR ({right})"))
            }
        }
    }
}
