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

#[derive(Debug, Clone)]
pub struct LazyQuery<M> {
    query: Query<M>,
}

#[derive(Debug, Clone)]
pub struct ChunkedQuery<M> {
    query: Query<M>,
    chunk_size: usize,
}

#[derive(Debug)]
struct LazyTryIter<M> {
    query: String,
    params: Vec<Value>,
    offset: usize,
    done: bool,
    __seekwel_model: PhantomData<M>,
}

#[derive(Debug)]
struct ChunkedTryIter<M> {
    query: String,
    params: Vec<Value>,
    chunk_size: usize,
    offset: usize,
    done: bool,
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

    pub fn lazy(self) -> LazyQuery<M> {
        LazyQuery { query: self }
    }

    pub fn chunked(self, chunk_size: usize) -> ChunkedQuery<M> {
        assert!(chunk_size > 0, "chunk size must be greater than 0");
        ChunkedQuery {
            query: self,
            chunk_size,
        }
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

    fn build_query(self, limit_one: bool) -> Result<(String, Vec<Value>), Error>
    where
        M: Model,
    {
        let mut params = Vec::new();
        let clause = self.expression.into_clause::<M>(&mut params)?;

        Ok((
            sql::select_where(M::table_name(), M::columns(), &clause, limit_one),
            params,
        ))
    }
}

impl<M: PersistedModel + 'static> Query<M> {
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

    pub fn iter(self) -> Result<Box<dyn Iterator<Item = M>>, Error> {
        Ok(Box::new(self.all()?.into_iter()))
    }

    pub fn try_iter(self) -> Result<Box<dyn Iterator<Item = M>>, Error> {
        self.iter()
    }
}

impl<M: PersistedModel + 'static> IntoIterator for Query<M> {
    type Item = M;
    type IntoIter = Box<dyn Iterator<Item = M>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
            .unwrap_or_else(|error| panic!("query iteration failed to start: {error}"))
    }
}

impl<M> LazyQuery<M> {
    pub fn q<T>(self, column: &str, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        Self {
            query: self.query.q(column, comparison),
        }
    }

    pub fn lazy(self) -> Self {
        self
    }

    pub fn chunked(self, chunk_size: usize) -> ChunkedQuery<M> {
        assert!(chunk_size > 0, "chunk size must be greater than 0");
        ChunkedQuery {
            query: self.query,
            chunk_size,
        }
    }

    pub fn and(self, other: Query<M>) -> Self {
        Self {
            query: self.query.and(other),
        }
    }

    pub fn or(self, other: Query<M>) -> Self {
        Self {
            query: self.query.or(other),
        }
    }
}

impl<M: PersistedModel + 'static> LazyQuery<M> {
    pub fn first(self) -> Result<Option<M>, Error> {
        self.query.first()
    }

    pub fn all(self) -> Result<Vec<M>, Error> {
        self.query.all()
    }

    pub fn iter(self) -> Result<Box<dyn Iterator<Item = M>>, Error> {
        let iter = self.try_iter()?.map(|result| {
            result.unwrap_or_else(|error| {
                panic!("lazy query iteration failed while fetching a row: {error}")
            })
        });
        Ok(Box::new(iter))
    }

    pub fn try_iter(self) -> Result<Box<dyn Iterator<Item = Result<M, Error>>>, Error> {
        let (query, params) = self.query.build_query(false)?;
        Ok(Box::new(LazyTryIter::new(query, params)))
    }
}

impl<M: PersistedModel + 'static> IntoIterator for LazyQuery<M> {
    type Item = M;
    type IntoIter = Box<dyn Iterator<Item = M>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
            .unwrap_or_else(|error| panic!("lazy query iteration failed to start: {error}"))
    }
}

impl<M> ChunkedQuery<M> {
    pub fn q<T>(self, column: &str, comparison: Comparison<T>) -> Self
    where
        T: ComparisonOperand,
    {
        Self {
            query: self.query.q(column, comparison),
            chunk_size: self.chunk_size,
        }
    }

    pub fn lazy(self) -> LazyQuery<M> {
        LazyQuery { query: self.query }
    }

    pub fn chunked(mut self, chunk_size: usize) -> Self {
        assert!(chunk_size > 0, "chunk size must be greater than 0");
        self.chunk_size = chunk_size;
        self
    }

    pub fn and(self, other: Query<M>) -> Self {
        Self {
            query: self.query.and(other),
            chunk_size: self.chunk_size,
        }
    }

    pub fn or(self, other: Query<M>) -> Self {
        Self {
            query: self.query.or(other),
            chunk_size: self.chunk_size,
        }
    }
}

impl<M: PersistedModel + 'static> ChunkedQuery<M> {
    pub fn first(self) -> Result<Option<M>, Error> {
        self.query.first()
    }

    pub fn all(self) -> Result<Vec<M>, Error> {
        self.query.all()
    }

    pub fn iter(self) -> Result<Box<dyn Iterator<Item = Vec<M>>>, Error> {
        let iter = self.try_iter()?.map(|result| {
            result.unwrap_or_else(|error| {
                panic!("chunked query iteration failed while fetching a chunk: {error}")
            })
        });
        Ok(Box::new(iter))
    }

    pub fn try_iter(self) -> Result<Box<dyn Iterator<Item = Result<Vec<M>, Error>>>, Error> {
        let (query, params) = self.query.build_query(false)?;
        Ok(Box::new(ChunkedTryIter::new(
            query,
            params,
            self.chunk_size,
        )))
    }
}

impl<M: PersistedModel + 'static> IntoIterator for ChunkedQuery<M> {
    type Item = Vec<M>;
    type IntoIter = Box<dyn Iterator<Item = Vec<M>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
            .unwrap_or_else(|error| panic!("chunked query iteration failed to start: {error}"))
    }
}

impl<M> LazyTryIter<M>
where
    M: PersistedModel,
{
    fn new(query: String, params: Vec<Value>) -> Self {
        Self {
            query,
            params,
            offset: 0,
            done: false,
            __seekwel_model: PhantomData,
        }
    }
}

impl<M> Iterator for LazyTryIter<M>
where
    M: PersistedModel,
{
    type Item = Result<M, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let query = format!("{} LIMIT 1 OFFSET {}", self.query, self.offset);
        let conn = match Connection::get() {
            Ok(conn) => conn,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };

        match conn.query_optional(&query, params_from_iter(self.params.clone()), M::from_row) {
            Ok(Some(model)) => {
                self.offset += 1;
                Some(Ok(model))
            }
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

impl<M> ChunkedTryIter<M>
where
    M: PersistedModel,
{
    fn new(query: String, params: Vec<Value>, chunk_size: usize) -> Self {
        Self {
            query,
            params,
            chunk_size,
            offset: 0,
            done: false,
            __seekwel_model: PhantomData,
        }
    }
}

impl<M> Iterator for ChunkedTryIter<M>
where
    M: PersistedModel,
{
    type Item = Result<Vec<M>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let query = format!(
            "{} LIMIT {} OFFSET {}",
            self.query, self.chunk_size, self.offset
        );
        let conn = match Connection::get() {
            Ok(conn) => conn,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };

        match conn.query_all(&query, params_from_iter(self.params.clone()), M::from_row) {
            Ok(rows) if rows.is_empty() => {
                self.done = true;
                None
            }
            Ok(rows) => {
                self.offset += rows.len();
                self.done = rows.len() < self.chunk_size;
                Some(Ok(rows))
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
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
