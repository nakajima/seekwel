use crate::model::ColumnDef;

use super::render::column_definitions;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CreateTable<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) columns: &'a [ColumnDef],
}

impl CreateTable<'_> {
    pub(crate) fn to_sql(self) -> String {
        let column_definitions = column_definitions(self.columns);

        if column_definitions.is_empty() {
            format!(
                "CREATE TABLE IF NOT EXISTS {} (id INTEGER PRIMARY KEY)",
                self.table_name
            )
        } else {
            format!(
                "CREATE TABLE IF NOT EXISTS {} (id INTEGER PRIMARY KEY, {})",
                self.table_name, column_definitions
            )
        }
    }
}

/// Builds a `CREATE TABLE IF NOT EXISTS` statement for a model table.
pub fn create_table(table_name: &str, columns: &[ColumnDef]) -> String {
    CreateTable {
        table_name,
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

    #[test]
    fn create_table_renders_columns() {
        assert_eq!(
            create_table("person", test_columns()),
            "CREATE TABLE IF NOT EXISTS person (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)"
        );
    }

    #[test]
    fn create_table_handles_empty_models() {
        assert_eq!(
            create_table("empty", &[]),
            "CREATE TABLE IF NOT EXISTS empty (id INTEGER PRIMARY KEY)"
        );
    }
}
