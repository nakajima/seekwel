use crate::model::ColumnDef;

pub(crate) fn column_definitions(columns: &[ColumnDef]) -> String {
    columns
        .iter()
        .map(|column| {
            if column.nullable {
                format!("{} {}", column.name, column.sql_type)
            } else {
                format!("{} {} NOT NULL", column.name, column.sql_type)
            }
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

pub(crate) fn select_columns(columns: &[ColumnDef]) -> String {
    let mut cols = vec!["id"];
    cols.extend(columns.iter().map(|column| column.name));
    cols.join(", ")
}
