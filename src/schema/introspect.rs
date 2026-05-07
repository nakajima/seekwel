use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{record_query, record_query_with_params};
use crate::error::Error;

use super::history;
use super::types::{ColumnDef, PrimaryKeyDef, SchemaDef, SqlAffinity, TableDef};

#[derive(Debug, Clone)]
pub(crate) struct ActualTable {
    pub(crate) table: TableDef,
    pub(crate) unsupported_inline_features: Vec<String>,
    pub(crate) has_real_foreign_keys: bool,
    pub(crate) dependent_views: Vec<String>,
    pub(crate) dependent_external_triggers: Vec<String>,
    pub(crate) replay_sql: Vec<ReplaySql>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplaySql {
    pub(crate) sql: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplayKind {
    Index,
    Trigger,
    View,
}

pub(crate) fn managed_scope(
    conn: &rusqlite::Connection,
    target: &SchemaDef,
) -> Result<Vec<String>, Error> {
    let mut names = BTreeSet::new();
    names.extend(target.table_names());

    if let Some(previous) = history::load_latest_target_schema(conn)? {
        names.extend(previous.table_names());
    }

    Ok(names.into_iter().collect())
}

pub(crate) fn introspect_managed(
    conn: &rusqlite::Connection,
    table_names: &[String],
) -> Result<BTreeMap<String, ActualTable>, Error> {
    let replayables = replayable_objects(conn)?;
    let dependency_candidates = dependency_candidates(conn)?;
    let all_tables = user_tables(conn)?;

    let mut tables = BTreeMap::new();
    for table_name in table_names {
        let Some(table_sql) = table_sql(conn, table_name)? else {
            continue;
        };

        let column_rows = table_columns(conn, table_name)?;
        let pk_count = column_rows.iter().filter(|column| column.pk > 0).count();
        let primary_key_column = column_rows.iter().find(|column| column.pk == 1);
        let primary_key_name = primary_key_column
            .map(|column| column.name.clone())
            .unwrap_or_else(|| "id".to_string());
        let primary_key_sql_type = primary_key_column
            .map(|column| column.sql_type.clone())
            .unwrap_or_else(|| "INTEGER".to_string());
        let primary_key_is_exact = primary_key_column.is_some_and(|column| {
            pk_count == 1
                && SqlAffinity::from_declared_type(&column.sql_type) == SqlAffinity::Integer
                && column.sql_type.trim().eq_ignore_ascii_case("INTEGER")
                && column.hidden == 0
        });

        let mut unsupported_inline_features = Vec::new();
        if !primary_key_is_exact {
            unsupported_inline_features.push(
                "managed tables must use a single `<name> INTEGER PRIMARY KEY` column".into(),
            );
        }
        if pk_count > 1 {
            unsupported_inline_features.push("composite primary keys are not supported".into());
        }
        if column_rows.iter().any(|column| column.hidden != 0) {
            unsupported_inline_features.push("generated or hidden columns are not supported".into());
        }
        if column_rows
            .iter()
            .any(|column| column.name != primary_key_name && column.default_sql.is_some())
        {
            unsupported_inline_features.push("default column values are not supported".into());
        }

        let normalized_sql = table_sql.to_ascii_uppercase();
        if normalized_sql.contains("CHECK") {
            unsupported_inline_features.push("CHECK constraints are not supported".into());
        }
        if normalized_sql.contains("WITHOUT ROWID") {
            unsupported_inline_features.push("WITHOUT ROWID tables are not supported".into());
        }
        if normalized_sql.contains("GENERATED ALWAYS") {
            unsupported_inline_features.push("generated columns are not supported".into());
        }

        let replay_sql = replayables.get(table_name).cloned().unwrap_or_default();
        let mut dependent_views = Vec::new();
        let mut dependent_external_triggers = Vec::new();

        for candidate in &dependency_candidates {
            if candidate.tbl_name == *table_name {
                continue;
            }
            if !sql_references_identifier(&candidate.sql, table_name) {
                continue;
            }

            match candidate.kind {
                ReplayKind::View => dependent_views.push(candidate.name.clone()),
                ReplayKind::Trigger => dependent_external_triggers.push(candidate.name.clone()),
                ReplayKind::Index => {}
            }
        }

        let has_real_foreign_keys = has_real_foreign_keys(conn, table_name, &all_tables)?;
        tables.insert(
            table_name.clone(),
            ActualTable {
                table: TableDef {
                    name: table_name.clone(),
                    primary_key: PrimaryKeyDef {
                        name: primary_key_name.clone(),
                        sql_type: primary_key_sql_type,
                    },
                    columns: column_rows
                        .into_iter()
                        .filter(|column| column.hidden == 0 && column.name != primary_key_name)
                        .map(|column| ColumnDef {
                            name: column.name,
                            sql_type: column.sql_type,
                            nullable: !column.not_null,
                        })
                        .collect(),
                },
                unsupported_inline_features,
                has_real_foreign_keys,
                dependent_views,
                dependent_external_triggers,
                replay_sql,
            },
        );
    }

    Ok(tables)
}

#[derive(Debug)]
struct ColumnRow {
    name: String,
    sql_type: String,
    not_null: bool,
    default_sql: Option<String>,
    pk: i64,
    hidden: i64,
}

fn table_sql(conn: &rusqlite::Connection, table_name: &str) -> Result<Option<String>, Error> {
    use rusqlite::OptionalExtension;

    let query = "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1";
    record_query_with_params(query, &[rusqlite::types::Value::Text(table_name.to_string())]);
    conn.query_row(
        query,
        [table_name],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map_err(Error::Sqlite)
    .map(|value| value.flatten())
}

fn table_columns(conn: &rusqlite::Connection, table_name: &str) -> Result<Vec<ColumnRow>, Error> {
    let pragma = format!("PRAGMA table_xinfo({})", pragma_string_arg(table_name));
    record_query(&pragma);
    let mut stmt = conn.prepare(&pragma).map_err(Error::Sqlite)?;
    let rows = stmt
        .query_map((), |row| {
            Ok(ColumnRow {
                name: row.get(1)?,
                sql_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                not_null: row.get::<_, i64>(3)? != 0,
                default_sql: row.get(4)?,
                pk: row.get(5)?,
                hidden: row.get(6)?,
            })
        })
        .map_err(Error::Sqlite)?;

    let mut columns = Vec::new();
    for row in rows {
        columns.push(row.map_err(Error::Sqlite)?);
    }
    Ok(columns)
}

fn replayable_objects(
    conn: &rusqlite::Connection,
) -> Result<BTreeMap<String, Vec<ReplaySql>>, Error> {
    let query = "SELECT name, type, tbl_name, sql FROM sqlite_schema WHERE type IN ('index', 'trigger') AND sql IS NOT NULL";
    record_query(query);
    let mut stmt = conn
        .prepare(query)
        .map_err(Error::Sqlite)?;

    let rows = stmt
        .query_map((), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(Error::Sqlite)?;

    let mut objects: BTreeMap<String, Vec<ReplaySql>> = BTreeMap::new();
    for row in rows {
        let (name, kind, table_name, sql) = row.map_err(Error::Sqlite)?;
        let kind = match kind.as_str() {
            "index" => ReplayKind::Index,
            "trigger" => ReplayKind::Trigger,
            _ => continue,
        };
        let _ = (name, kind);
        objects.entry(table_name).or_default().push(ReplaySql { sql });
    }

    Ok(objects)
}

fn dependency_candidates(conn: &rusqlite::Connection) -> Result<Vec<ReplaySqlWithTable>, Error> {
    let query = "SELECT name, type, tbl_name, sql FROM sqlite_schema WHERE type IN ('view', 'trigger') AND sql IS NOT NULL";
    record_query(query);
    let mut stmt = conn
        .prepare(query)
        .map_err(Error::Sqlite)?;

    let rows = stmt
        .query_map((), |row| {
            Ok(ReplaySqlWithTable {
                name: row.get(0)?,
                kind: match row.get::<_, String>(1)?.as_str() {
                    "view" => ReplayKind::View,
                    "trigger" => ReplayKind::Trigger,
                    _ => unreachable!(),
                },
                tbl_name: row.get(2)?,
                sql: row.get(3)?,
            })
        })
        .map_err(Error::Sqlite)?;

    let mut objects = Vec::new();
    for row in rows {
        objects.push(row.map_err(Error::Sqlite)?);
    }
    Ok(objects)
}

#[derive(Debug, Clone)]
struct ReplaySqlWithTable {
    name: String,
    kind: ReplayKind,
    tbl_name: String,
    sql: String,
}

fn user_tables(conn: &rusqlite::Connection) -> Result<Vec<String>, Error> {
    let query = "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != ?1";
    record_query_with_params(
        query,
        &[rusqlite::types::Value::Text(super::types::HISTORY_TABLE.to_string())],
    );
    let mut stmt = conn
        .prepare(query)
        .map_err(Error::Sqlite)?;
    let rows = stmt
        .query_map([super::types::HISTORY_TABLE], |row| row.get::<_, String>(0))
        .map_err(Error::Sqlite)?;

    let mut names = Vec::new();
    for row in rows {
        names.push(row.map_err(Error::Sqlite)?);
    }
    Ok(names)
}

fn has_real_foreign_keys(
    conn: &rusqlite::Connection,
    table_name: &str,
    all_tables: &[String],
) -> Result<bool, Error> {
    if foreign_key_count(conn, table_name)? > 0 {
        return Ok(true);
    }

    for other in all_tables {
        if other == table_name {
            continue;
        }
        let pragma = format!("PRAGMA foreign_key_list({})", pragma_string_arg(other));
        record_query(&pragma);
        let mut stmt = conn.prepare(&pragma).map_err(Error::Sqlite)?;
        let rows = stmt
            .query_map((), |row| row.get::<_, String>(2))
            .map_err(Error::Sqlite)?;
        for row in rows {
            if row.map_err(Error::Sqlite)?.eq_ignore_ascii_case(table_name) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn foreign_key_count(conn: &rusqlite::Connection, table_name: &str) -> Result<usize, Error> {
    let pragma = format!("PRAGMA foreign_key_list({})", pragma_string_arg(table_name));
    record_query(&pragma);
    let mut stmt = conn.prepare(&pragma).map_err(Error::Sqlite)?;
    let rows = stmt.query_map((), |_| Ok(())).map_err(Error::Sqlite)?;
    Ok(rows.count())
}

fn pragma_string_arg(identifier: &str) -> String {
    format!("'{}'", identifier.replace('\'', "''"))
}

pub(crate) fn sql_references_identifier(sql: &str, identifier: &str) -> bool {
    let identifier = identifier.to_ascii_lowercase();
    let sql_lower = sql.to_ascii_lowercase();

    if sql_lower.contains(&format!("\"{identifier}\""))
        || sql_lower.contains(&format!("[{identifier}]"))
        || sql_lower.contains(&format!("`{identifier}`"))
    {
        return true;
    }

    let mut token = String::new();
    for ch in sql_lower.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch);
        } else {
            if token == identifier {
                return true;
            }
            token.clear();
        }
    }

    token == identifier
}
