use crate::model::{ColumnDef, PrimaryKeyDef};

use super::render::column_definitions;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CreateTable<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) primary_key: PrimaryKeyDef,
    pub(crate) columns: &'a [ColumnDef],
}

impl CreateTable<'_> {
    pub(crate) fn to_sql(self) -> String {
        let column_definitions = column_definitions(self.columns);
        let primary_key = format!("{} {} PRIMARY KEY", self.primary_key.name, self.primary_key.sql_type);

        if column_definitions.is_empty() {
            format!(
                "CREATE TABLE IF NOT EXISTS {} ({})",
                self.table_name, primary_key
            )
        } else {
            format!(
                "CREATE TABLE IF NOT EXISTS {} ({}, {})",
                self.table_name, primary_key, column_definitions
            )
        }
    }
}

/// Builds a `CREATE TABLE IF NOT EXISTS` statement for a model table.
pub fn create_table(table_name: &str, primary_key: PrimaryKeyDef, columns: &[ColumnDef]) -> String {
    CreateTable {
        table_name,
        primary_key,
        columns,
    }
    .to_sql()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_columns() -> &'static [ColumnDef] {
        &[
            ColumnDef {
                name: "name",
                sql_type: "TEXT",
                nullable: false,
            },
            ColumnDef {
                name: "age",
                sql_type: "INTEGER",
                nullable: true,
            },
        ]
    }

    fn primary_key(name: &'static str) -> PrimaryKeyDef {
        PrimaryKeyDef {
            name,
            sql_type: "INTEGER",
            auto_increment: true,
        }
    }

    #[test]
    fn create_table_renders_columns() {
        assert_eq!(
            create_table("person", primary_key("id"), test_columns()),
            "CREATE TABLE IF NOT EXISTS person (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)"
        );
    }

    #[test]
    fn create_table_handles_empty_models() {
        assert_eq!(
            create_table("empty", primary_key("hyperlink_id"), &[]),
            "CREATE TABLE IF NOT EXISTS empty (hyperlink_id INTEGER PRIMARY KEY)"
        );
    }
}
