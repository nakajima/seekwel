use rusqlite::types::Value;

use crate::error::Error;

use super::super::comparison::PreparedComparison;

#[derive(Debug, Clone)]
pub(super) enum QueryExpression {
    Empty,
    Predicate {
        column: &'static str,
        comparison: PreparedComparison,
    },
    And(Box<QueryExpression>, Box<QueryExpression>),
    Or(Box<QueryExpression>, Box<QueryExpression>),
}

impl QueryExpression {
    pub(super) fn into_clause(self, params: &mut Vec<Value>) -> Result<Option<String>, Error> {
        match self {
            QueryExpression::Empty => Ok(None),
            QueryExpression::Predicate { column, comparison } => {
                comparison.into_clause(column, params).map(Some)
            }
            QueryExpression::And(left, right) => {
                let left = left.into_clause(params)?;
                let right = right.into_clause(params)?;
                Ok(combine_clauses(left, right, "AND"))
            }
            QueryExpression::Or(left, right) => {
                let left = left.into_clause(params)?;
                let right = right.into_clause(params)?;
                Ok(combine_clauses(left, right, "OR"))
            }
        }
    }
}

fn combine_clauses(left: Option<String>, right: Option<String>, operator: &str) -> Option<String> {
    match (left, right) {
        (None, None) => None,
        (Some(clause), None) | (None, Some(clause)) => Some(clause),
        (Some(left), Some(right)) => Some(format!("({left}) {operator} ({right})")),
    }
}
