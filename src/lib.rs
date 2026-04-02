extern crate self as seekwel;

pub mod connection;
pub mod error;
pub mod model;
pub mod sql;

pub use model::{Comparison, NewRecord, Persisted, Query, SqlField};
pub use seekwel_derive::{Model, model};
