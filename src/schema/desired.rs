use crate::error::Error;
use crate::model;

use super::plan::Plan;
use super::registry;
use super::types::{ColumnDef, PrimaryKeyDef, SchemaDef, TableDef};

/// Builds a desired managed schema from one or more seekwel models.
#[derive(Debug, Clone, Default)]
pub struct SchemaBuilder {
    tables: Vec<TableDef>,
}

impl SchemaBuilder {
    /// Creates an empty desired schema builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a managed model table to the desired schema.
    pub fn model<M: model::Model>(mut self) -> Self {
        self.tables.push(table_for_model::<M>());
        self
    }

    /// Builds a desired schema from all automatically registered seekwel models.
    pub fn registered() -> Result<Self, Error> {
        Ok(Self {
            tables: registry::registered_tables()?,
        })
    }

    /// Finalizes the desired schema in deterministic order.
    pub fn build(self) -> Result<SchemaDef, Error> {
        SchemaDef { tables: self.tables }.normalized()
    }

    /// Builds a reviewable schema plan against the live database.
    pub fn plan(self) -> Result<Plan, Error> {
        Plan::build(self.build()?)
    }
}

pub(crate) fn table_for_model<M: model::Model>() -> TableDef {
    TableDef {
        name: M::table_name().to_string(),
        primary_key: {
            let primary_key = M::primary_key();
            PrimaryKeyDef {
                name: primary_key.name.to_string(),
                sql_type: primary_key.sql_type.to_string(),
            }
        },
        columns: M::columns()
            .iter()
            .map(|column| ColumnDef {
                name: column.name.to_string(),
                sql_type: column.sql_type.to_string(),
                nullable: column.nullable,
            })
            .collect(),
    }
}
