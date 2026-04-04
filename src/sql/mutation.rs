use crate::model::ColumnDef;

use super::render::{assignments, column_names, placeholders};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Insert<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) columns: &'a [ColumnDef],
}

impl Insert<'_> {
    pub(crate) fn to_sql(self) -> String {
        if self.columns.is_empty() {
            return format!("INSERT INTO {} DEFAULT VALUES", self.table_name);
        }

        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table_name,
            column_names(self.columns),
            placeholders(self.columns.len(), 1)
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct UpdateById<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) columns: &'a [ColumnDef],
}

impl UpdateById<'_> {
    pub(crate) fn to_sql(self) -> String {
        if self.columns.is_empty() {
            return format!("UPDATE {} SET id = id WHERE id = ?1", self.table_name);
        }

        let id_placeholder = self.columns.len() + 1;
        format!(
            "UPDATE {} SET {} WHERE id = ?{}",
            self.table_name,
            assignments(self.columns, 1),
            id_placeholder
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DeleteById<'a> {
    pub(crate) table_name: &'a str,
}

impl DeleteById<'_> {
    pub(crate) fn to_sql(self) -> String {
        format!("DELETE FROM {} WHERE id = ?1", self.table_name)
    }
}

/// Builds an `INSERT` statement for a model table.
pub fn insert(table_name: &str, columns: &[ColumnDef]) -> String {
    Insert {
        table_name,
        columns,
    }
    .to_sql()
}

/// Builds an `UPDATE ... WHERE id = ?n` statement for a model table.
pub fn update_by_id(table_name: &str, columns: &[ColumnDef]) -> String {
    UpdateById {
        table_name,
        columns,
    }
    .to_sql()
}

/// Builds a `DELETE ... WHERE id = ?1` statement for a model table.
pub fn delete_by_id(table_name: &str) -> String {
    DeleteById { table_name }.to_sql()
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
    fn insert_renders_placeholders() {
        assert_eq!(
            insert("person", test_columns()),
            "INSERT INTO person (name, age) VALUES (?1, ?2)"
        );
    }

    #[test]
    fn insert_handles_empty_models() {
        assert_eq!(insert("person", &[]), "INSERT INTO person DEFAULT VALUES");
    }

    #[test]
    fn update_by_id_renders_assignments() {
        assert_eq!(
            update_by_id("person", test_columns()),
            "UPDATE person SET name = ?1, age = ?2 WHERE id = ?3"
        );
    }

    #[test]
    fn update_by_id_handles_empty_models() {
        assert_eq!(
            update_by_id("empty", &[]),
            "UPDATE empty SET id = id WHERE id = ?1"
        );
    }

    #[test]
    fn delete_by_id_renders_statement() {
        assert_eq!(delete_by_id("person"), "DELETE FROM person WHERE id = ?1");
    }
}
