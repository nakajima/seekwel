use crate::model::{ColumnDef, PrimaryKeyDef};

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
pub(crate) struct UpdateByPrimaryKey<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) primary_key: PrimaryKeyDef,
    pub(crate) columns: &'a [ColumnDef],
}

impl UpdateByPrimaryKey<'_> {
    pub(crate) fn to_sql(self) -> String {
        if self.columns.is_empty() {
            return format!(
                "UPDATE {} SET {} = {} WHERE {} = ?1",
                self.table_name,
                self.primary_key.name,
                self.primary_key.name,
                self.primary_key.name,
            );
        }

        let id_placeholder = self.columns.len() + 1;
        format!(
            "UPDATE {} SET {} WHERE {} = ?{}",
            self.table_name,
            assignments(self.columns, 1),
            self.primary_key.name,
            id_placeholder
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DeleteByPrimaryKey<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) primary_key: PrimaryKeyDef,
}

impl DeleteByPrimaryKey<'_> {
    pub(crate) fn to_sql(self) -> String {
        format!(
            "DELETE FROM {} WHERE {} = ?1",
            self.table_name, self.primary_key.name
        )
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
pub fn update_by_primary_key(
    table_name: &str,
    primary_key: PrimaryKeyDef,
    columns: &[ColumnDef],
) -> String {
    UpdateByPrimaryKey {
        table_name,
        primary_key,
        columns,
    }
    .to_sql()
}

/// Builds a `DELETE ... WHERE <pk> = ?1` statement for a model table.
pub fn delete_by_primary_key(table_name: &str, primary_key: PrimaryKeyDef) -> String {
    DeleteByPrimaryKey {
        table_name,
        primary_key,
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

    fn primary_key(name: &'static str) -> PrimaryKeyDef {
        PrimaryKeyDef {
            name,
            sql_type: "INTEGER",
            auto_increment: true,
        }
    }

    #[test]
    fn update_by_primary_key_renders_assignments() {
        assert_eq!(
            update_by_primary_key("person", primary_key("id"), test_columns()),
            "UPDATE person SET name = ?1, age = ?2 WHERE id = ?3"
        );
    }

    #[test]
    fn update_by_primary_key_handles_empty_models() {
        assert_eq!(
            update_by_primary_key("empty", primary_key("hyperlink_id"), &[]),
            "UPDATE empty SET hyperlink_id = hyperlink_id WHERE hyperlink_id = ?1"
        );
    }

    #[test]
    fn delete_by_primary_key_renders_statement() {
        assert_eq!(
            delete_by_primary_key("person", primary_key("id")),
            "DELETE FROM person WHERE id = ?1"
        );
    }
}
