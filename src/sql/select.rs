use crate::model::ColumnDef;

use super::render::select_columns;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrderDirection {
    Asc,
    Desc,
}

impl OrderDirection {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OrderTerm {
    Column {
        name: &'static str,
        direction: OrderDirection,
    },
    Raw(String),
}

impl OrderTerm {
    pub(crate) fn to_sql(&self) -> String {
        match self {
            Self::Column { name, direction } => format!("{name} {}", direction.as_sql()),
            Self::Raw(sql) => sql.clone(),
        }
    }
}

pub(crate) fn order_by_clause(ordering: &[OrderTerm]) -> Option<String> {
    if ordering.is_empty() {
        return None;
    }

    Some(
        ordering
            .iter()
            .map(OrderTerm::to_sql)
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Projection<'a> {
    ModelColumns(&'a [ColumnDef]),
    CountAll,
    One,
}

impl Projection<'_> {
    fn to_sql(self) -> String {
        match self {
            Self::ModelColumns(columns) => select_columns(columns),
            Self::CountAll => "COUNT(*)".to_string(),
            Self::One => "1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Select<'a> {
    pub(crate) projection: Projection<'a>,
    pub(crate) table_name: &'a str,
    pub(crate) clause: Option<&'a str>,
    pub(crate) order_clause: Option<&'a str>,
    pub(crate) limit: Option<usize>,
    pub(crate) offset: Option<usize>,
}

impl Select<'_> {
    pub(crate) fn to_sql(self) -> String {
        let mut query = format!(
            "SELECT {} FROM {}",
            self.projection.to_sql(),
            self.table_name
        );

        if let Some(clause) = self.clause {
            query.push_str(&format!(" WHERE {clause}"));
        }

        if let Some(order_clause) = self.order_clause {
            query.push_str(&format!(" ORDER BY {order_clause}"));
        }

        append_limit_offset(&query, self.limit, self.offset)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Count<'a> {
    pub(crate) select: Select<'a>,
}

impl Count<'_> {
    pub(crate) fn to_sql(self) -> String {
        if self.select.limit.is_none() && self.select.offset.is_none() {
            return Select {
                projection: Projection::CountAll,
                order_clause: None,
                ..self.select
            }
            .to_sql();
        }

        let inner = Select {
            projection: Projection::One,
            ..self.select
        }
        .to_sql();

        format!("SELECT COUNT(*) FROM ({inner}) AS seekwel_count")
    }
}

/// Builds a `SELECT ... WHERE id = ?1` statement for a model table.
pub(crate) fn select_by_id(table_name: &str, columns: &[ColumnDef]) -> String {
    Select {
        projection: Projection::ModelColumns(columns),
        table_name,
        clause: Some("id = ?1"),
        order_clause: None,
        limit: None,
        offset: None,
    }
    .to_sql()
}

/// Builds a `SELECT` statement with optional `WHERE`, `ORDER BY`, `LIMIT`, and
/// `OFFSET` clauses.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn select_with_options(
    table_name: &str,
    columns: &[ColumnDef],
    clause: Option<&str>,
    order_clause: Option<&str>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> String {
    Select {
        projection: Projection::ModelColumns(columns),
        table_name,
        clause,
        order_clause,
        limit,
        offset,
    }
    .to_sql()
}

/// Builds a `SELECT` statement with an optional `WHERE` clause and optional
/// `LIMIT 1`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn select(
    table_name: &str,
    columns: &[ColumnDef],
    clause: Option<&str>,
    limit_one: bool,
) -> String {
    select_with_options(
        table_name,
        columns,
        clause,
        None,
        limit_one.then_some(1),
        None,
    )
}

/// Builds a `SELECT` statement with a required `WHERE` clause.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn select_where(
    table_name: &str,
    columns: &[ColumnDef],
    clause: &str,
    limit_one: bool,
) -> String {
    select(table_name, columns, Some(clause), limit_one)
}

/// Appends `LIMIT` and `OFFSET` clauses to an existing `SELECT` statement.
///
/// SQLite requires a `LIMIT` when using `OFFSET`, so `LIMIT -1` is emitted for
/// offset-only queries.
pub(crate) fn append_limit_offset(
    query: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> String {
    let mut query = query.to_string();

    if let Some(limit) = limit {
        query.push_str(&format!(" LIMIT {limit}"));
    }

    if let Some(offset) = offset {
        if limit.is_none() {
            query.push_str(" LIMIT -1");
        }
        query.push_str(&format!(" OFFSET {offset}"));
    }

    query
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
    fn select_by_id_renders_lookup() {
        assert_eq!(
            select_by_id("person", test_columns()),
            "SELECT id, name, age FROM person WHERE id = ?1"
        );
    }

    #[test]
    fn select_where_renders_required_clause() {
        assert_eq!(
            select_where("person", test_columns(), "age >= ?1", false),
            "SELECT id, name, age FROM person WHERE age >= ?1"
        );
    }

    #[test]
    fn select_renders_unfiltered_queries() {
        assert_eq!(
            select("person", test_columns(), None, false),
            "SELECT id, name, age FROM person"
        );
    }

    #[test]
    fn select_where_applies_limit_one() {
        assert_eq!(
            select_where("person", test_columns(), "name = ?1", true),
            "SELECT id, name, age FROM person WHERE name = ?1 LIMIT 1"
        );
    }

    #[test]
    fn select_applies_limit_one() {
        assert_eq!(
            select("person", test_columns(), None, true),
            "SELECT id, name, age FROM person LIMIT 1"
        );
    }

    #[test]
    fn select_with_options_renders_order_and_paging() {
        assert_eq!(
            select_with_options(
                "person",
                test_columns(),
                Some("age >= ?1"),
                Some("name ASC, age DESC"),
                Some(10),
                Some(20),
            ),
            "SELECT id, name, age FROM person WHERE age >= ?1 ORDER BY name ASC, age DESC LIMIT 10 OFFSET 20"
        );
    }

    #[test]
    fn count_without_paging_ignores_ordering() {
        assert_eq!(
            Count {
                select: Select {
                    projection: Projection::One,
                    table_name: "person",
                    clause: Some("age >= ?1"),
                    order_clause: Some("name DESC"),
                    limit: None,
                    offset: None,
                },
            }
            .to_sql(),
            "SELECT COUNT(*) FROM person WHERE age >= ?1"
        );
    }

    #[test]
    fn count_with_paging_wraps_the_query() {
        assert_eq!(
            Count {
                select: Select {
                    projection: Projection::One,
                    table_name: "person",
                    clause: Some("age >= ?1"),
                    order_clause: Some("name DESC"),
                    limit: Some(10),
                    offset: Some(20),
                },
            }
            .to_sql(),
            "SELECT COUNT(*) FROM (SELECT 1 FROM person WHERE age >= ?1 ORDER BY name DESC LIMIT 10 OFFSET 20) AS seekwel_count"
        );
    }

    #[test]
    fn append_limit_offset_handles_offset_only() {
        assert_eq!(
            append_limit_offset("SELECT id FROM person", None, Some(20)),
            "SELECT id FROM person LIMIT -1 OFFSET 20"
        );
    }

    #[test]
    fn order_by_clause_joins_terms() {
        let ordering = vec![
            OrderTerm::Column {
                name: "name",
                direction: OrderDirection::Asc,
            },
            OrderTerm::Raw("age DESC".to_string()),
        ];

        assert_eq!(
            order_by_clause(&ordering),
            Some("name ASC, age DESC".to_string())
        );
    }
}
