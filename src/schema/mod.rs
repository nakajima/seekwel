//! Schema planning and migration helpers.
//!
//! This module provides the first building blocks for safe schema
//! reconciliation in seekwel.
//!
//! Use [`SchemaBuilder`] to assemble a desired managed schema from models,
//! then call [`SchemaBuilder::plan`] to compare it against the live database.

mod apply;
mod desired;
mod diff;
mod history;
mod introspect;
mod plan;
mod registry;
mod types;

pub use desired::SchemaBuilder;
pub use plan::{ApplyMode, Plan, PlanBlocker, PlanOp, RebuildReason};
pub use types::{ColumnDef, PrimaryKeyDef, SchemaDef, SqlAffinity, TableDef};

#[doc(hidden)]
pub mod __private {
    pub use super::registry::RegistryEntry;
    pub fn table_for_model<M: crate::model::Model>() -> super::TableDef {
        super::desired::table_for_model::<M>()
    }
}
