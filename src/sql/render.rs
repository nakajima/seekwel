use crate::model::ColumnDef;

pub(crate) fn column_definitions(columns: &[ColumnDef]) -> String {
    columns
        .iter()
        .map(|column| {
            let mut definition = format!("{} {}", column.name, column.sql_type);
            if !column.nullable {
                definition.push_str(" NOT NULL");
            }
            if let Some(default_sql) = column.default_sql {
                definition.push_str(" DEFAULT ");
                definition.push_str(default_sql);
            }
            definition
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn column_names(columns: &[ColumnDef]) -> String {
    columns
        .iter()
        .map(|column| column.name)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn placeholders(count: usize, start_at: usize) -> String {
    (start_at..start_at + count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn assignments(columns: &[ColumnDef], start_at: usize) -> String {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| format!("{} = ?{}", column.name, start_at + index))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn select_columns(primary_key_name: &str, columns: &[ColumnDef]) -> String {
    let mut cols = vec![primary_key_name];
    cols.extend(columns.iter().map(|column| column.name));
    cols.join(", ")
}
