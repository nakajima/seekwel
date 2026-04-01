use crate::model::ColumnDef;

pub fn create_table(table_name: &str, columns: &[ColumnDef]) -> String {
    let col_defs: Vec<String> = columns
        .iter()
        .map(|c| {
            if c.nullable {
                format!("{} {}", c.name, c.sql_type)
            } else {
                format!("{} {} NOT NULL", c.name, c.sql_type)
            }
        })
        .collect();

    if col_defs.is_empty() {
        format!("CREATE TABLE IF NOT EXISTS {table_name} (id INTEGER PRIMARY KEY)")
    } else {
        format!(
            "CREATE TABLE IF NOT EXISTS {table_name} (id INTEGER PRIMARY KEY, {})",
            col_defs.join(", ")
        )
    }
}

pub fn insert(table_name: &str, columns: &[ColumnDef]) -> String {
    if columns.is_empty() {
        return format!("INSERT INTO {table_name} DEFAULT VALUES");
    }

    let col_names = columns
        .iter()
        .map(|c| c.name)
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=columns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("INSERT INTO {table_name} ({col_names}) VALUES ({placeholders})")
}

pub fn select_by_id(table_name: &str, columns: &[ColumnDef]) -> String {
    let mut cols = vec!["id"];
    cols.extend(columns.iter().map(|c| c.name));
    format!("SELECT {} FROM {table_name} WHERE id = ?1", cols.join(", "))
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
    fn test_create_table() {
        assert_eq!(
            create_table("person", test_columns()),
            "CREATE TABLE IF NOT EXISTS person (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)"
        );
    }

    #[test]
    fn test_create_table_no_columns() {
        assert_eq!(
            create_table("empty", &[]),
            "CREATE TABLE IF NOT EXISTS empty (id INTEGER PRIMARY KEY)"
        );
    }

    #[test]
    fn test_insert() {
        assert_eq!(
            insert("person", test_columns()),
            "INSERT INTO person (name, age) VALUES (?1, ?2)"
        );
    }

    #[test]
    fn test_insert_no_columns() {
        assert_eq!(insert("person", &[]), "INSERT INTO person DEFAULT VALUES");
    }

    #[test]
    fn test_select_by_id() {
        assert_eq!(
            select_by_id("person", test_columns()),
            "SELECT id, name, age FROM person WHERE id = ?1"
        );
    }
}
