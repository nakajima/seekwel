extern crate self as seekwel;

pub mod connection;
pub mod error;
pub mod model;
pub mod sql;

pub use model::{NewRecord, Persisted};
pub use seekwel_derive::{Model, model};
