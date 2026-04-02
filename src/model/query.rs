use std::marker::PhantomData;

use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::connection::Connection;
use crate::error::Error;
use crate::sql;

use super::comparison::PreparedComparison;
use super::{Comparison, ComparisonOperand, Model, PersistedModel};

#[derive(Debug, Clone)]
enum QueryExpression {
    Predicate {
        column: String,
        comparison: PreparedComparison,
    },
    And(Box<QueryExpression>, Box<QueryExpression>),
    Or(Box<QueryExpression>, Box<QueryExpression>),
}

#[derive(Debug, Clone)]
pub struct Query<M> {
    expression: QueryExpression,
    __seekwel_model: PhantomData<M>,
}

impl<M> Query<M> {
    pub fn new<T>(column: &str, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        Self {
            expression: QueryExpression::Predicate {
                column: column.to_string(),
                comparison: comparison.into_prepared(),
            },
            __seekwel_model: PhantomData,
        }
    }

    pub fn q<T>(self, column: &str, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        self.and(Self::new(column, comparison))
    }

    pub fn and(self, other: Self) -> Self {
        Self {
            expression: QueryExpression::And(Box::new(self.expression), Box::new(other.expression)),
            __seekwel_model: PhantomData,
        }
    }

    pub fn or(self, other: Self) -> Self {
        Self {
            expression: QueryExpression::Or(Box::new(self.expression), Box::new(other.expression)),
            __seekwel_model: PhantomData,
        }
    }
}

impl<M: PersistedModel> Query<M> {
    pub fn first(self) -> Result<Option<M>, Error> {
        let conn = Connection::get()?;
        let (query, params) = self.build_query(true)?;
        conn.query_optional(&query, params_from_iter(params), M::from_row)
    }

    pub fn all(self) -> Result<Vec<M>, Error> {
        let conn = Connection::get()?;
        let (query, params) = self.build_query(false)?;
        conn.query_all(&query, params_from_iter(params), M::from_row)
    }

    fn build_query(self, limit_one: bool) -> Result<(String, Vec<Value>), Error> {
        let mut params = Vec::new();
        let clause = self.expression.into_clause::<M>(&mut params)?;

        Ok((
            sql::select_where(M::table_name(), M::columns(), &clause, limit_one),
            params,
        ))
    }
}

impl QueryExpression {
    fn into_clause<M: Model>(self, params: &mut Vec<Value>) -> Result<String, Error> {
        match self {
            QueryExpression::Predicate { column, comparison } => {
                super::validate_column::<M>(&column)?;
                comparison.into_clause(&column, params)
            }
            QueryExpression::And(left, right) => {
                let left = left.into_clause::<M>(params)?;
                let right = right.into_clause::<M>(params)?;
                Ok(format!("({left}) AND ({right})"))
            }
            QueryExpression::Or(left, right) => {
                let left = left.into_clause::<M>(params)?;
                let right = right.into_clause::<M>(params)?;
                Ok(format!("({left}) OR ({right})"))
            }
        }
    }
}
