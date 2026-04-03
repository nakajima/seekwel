extern crate self as seekwel;

pub mod connection;
pub mod error;
pub mod model;
pub mod sql;

pub use model::{
    Chunked, ChunkedIter, ChunkedQuery, ChunkedTryIter, Comparison, Lazy, LazyIter, LazyQuery,
    LazyTryIter, NewRecord, Persisted, Query, QueryDsl, SqlField,
};
pub use seekwel_derive::{Model, model};

pub mod prelude {
    pub use crate::QueryDsl;
}
