use std::collections::BTreeSet;

use rusqlite::TransactionBehavior;

use crate::connection::{Connection, record_query};
use crate::error::Error;

use super::diff;
use super::history;
use super::introspect::{self, ActualTable};
use super::plan::{ApplyMode, Plan, PlanOp};
use super::types::{ColumnDef, TableDef};

pub(crate) fn apply(plan: &Plan, mode: ApplyMode) -> Result<(), Error> {
    let mut conn = Connection::get()?;
    let scope = plan_scope(plan);
    let actual = introspect::introspect_managed(conn.raw(), &scope)?;
    let current = diff::diff(&plan.target, &actual)?;
    let current_source_fingerprint = current.source.fingerprint();

    if current_source_fingerprint != plan.source_fingerprint {
        return Err(Error::SchemaDrift(format!(
            "expected source fingerprint {}, found {}",
            plan.source_fingerprint, current_source_fingerprint
        )));
    }

    if current.ops != plan.ops {
        return Err(Error::SchemaBlocked(
            "live schema still matches the same source fingerprint, but the planned operations changed; rebuild a fresh plan before applying".into(),
        ));
    }

    if !current.blockers.is_empty() {
        return Err(Error::SchemaBlocked(format_blockers(&current.blockers)));
    }

    if mode == ApplyMode::SafeOnly && current.ops.iter().any(PlanOp::is_destructive) {
        return Err(Error::SchemaBlocked(
            "plan contains destructive operations; re-run apply with ApplyMode::AllowDestructive".into(),
        ));
    }

    record_query("BEGIN IMMEDIATE");
    let tx = conn
        .raw_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(Error::Sqlite)?;

    history::ensure_history_table(&tx)?;

    for op in &current.ops {
        match op {
            PlanOp::CreateTable { table } => create_table(&tx, &table.name, table)?,
            PlanOp::AddColumn { table, column } => add_column(&tx, table, column)?,
            PlanOp::RebuildTable { table, from, to, .. } => {
                let actual = actual.get(table).ok_or_else(|| {
                    Error::SchemaBlocked(format!(
                        "table `{table}` disappeared before rebuild could run"
                    ))
                })?;
                rebuild_table(&tx, table, from, to, actual)?;
            }
            PlanOp::DropTable { table } => drop_table(&tx, &table.name)?,
        }
    }

    record_query("PRAGMA integrity_check");
    let integrity = tx
        .query_row("PRAGMA integrity_check", (), |row| row.get::<_, String>(0))
        .map_err(Error::Sqlite)?;
    if integrity != "ok" {
        return Err(Error::SchemaBlocked(format!(
            "integrity_check failed after schema apply: {integrity}"
        )));
    }

    history::record_success(
        &tx,
        &plan.plan_id,
        &plan.source_fingerprint,
        &plan.target_fingerprint,
        &plan.target,
    )?;

    tx.commit().map_err(Error::Sqlite)?;
    conn.execute_batch("PRAGMA optimize")?;
    Ok(())
}

fn plan_scope(plan: &Plan) -> Vec<String> {
    let mut names = BTreeSet::new();
    names.extend(plan.source.table_names());
    names.extend(plan.target.table_names());
    names.into_iter().collect()
}

fn create_table(
    tx: &rusqlite::Transaction<'_>,
    table_name: &str,
    table: &TableDef,
) -> Result<(), Error> {
    let sql = render_create_table_sql(table_name, table);
    record_query(&sql);
    tx.execute_batch(&sql).map_err(Error::Sqlite)
}

fn add_column(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &ColumnDef,
) -> Result<(), Error> {
    let sql = render_add_column_sql(table, column);
    record_query(&sql);
    tx.execute_batch(&sql).map_err(Error::Sqlite)
}

fn drop_table(tx: &rusqlite::Transaction<'_>, table: &str) -> Result<(), Error> {
    let sql = format!("DROP TABLE {}", quote_ident(table));
    record_query(&sql);
    tx.execute_batch(&sql).map_err(Error::Sqlite)
}

fn rebuild_table(
    tx: &rusqlite::Transaction<'_>,
    table_name: &str,
    from: &TableDef,
    to: &TableDef,
    actual: &ActualTable,
) -> Result<(), Error> {
    let temp_name = format!("__seekwel_rebuild_{table_name}");
    let create_sql = render_create_table_sql(&temp_name, to);
    record_query(&create_sql);
    tx.execute_batch(&create_sql).map_err(Error::Sqlite)?;

    let retained_columns = retained_columns(from, to);
    let copy_sql = render_copy_sql(table_name, &temp_name, &retained_columns);
    record_query(&copy_sql);
    tx.execute_batch(&copy_sql).map_err(Error::Sqlite)?;

    let drop_sql = format!("DROP TABLE {}", quote_ident(table_name));
    record_query(&drop_sql);
    tx.execute_batch(&drop_sql).map_err(Error::Sqlite)?;

    let rename_sql = format!(
        "ALTER TABLE {} RENAME TO {}",
        quote_ident(&temp_name),
        quote_ident(table_name)
    );
    record_query(&rename_sql);
    tx.execute_batch(&rename_sql).map_err(Error::Sqlite)?;

    for replay in &actual.replay_sql {
        record_query(&replay.sql);
        tx.execute_batch(&replay.sql).map_err(Error::Sqlite)?;
    }

    Ok(())
}

fn retained_columns(from: &TableDef, to: &TableDef) -> Vec<(String, String)> {
    let to_columns: BTreeSet<_> = to.columns.iter().map(|column| column.name.as_str()).collect();
    let mut retained = vec![(from.primary_key.name.clone(), to.primary_key.name.clone())];
    retained.extend(
        from.columns
            .iter()
            .map(|column| column.name.as_str())
            .filter(|name| to_columns.contains(name))
            .map(|name| (name.to_string(), name.to_string())),
    );
    retained
}

fn render_create_table_sql(table_name: &str, table: &TableDef) -> String {
    let mut defs = vec![format!(
        "{} {} PRIMARY KEY",
        quote_ident(&table.primary_key.name),
        table.primary_key.sql_type
    )];
    defs.extend(table.columns.iter().map(render_column_definition));
    format!("CREATE TABLE {} ({})", quote_ident(table_name), defs.join(", "))
}

fn render_add_column_sql(table_name: &str, column: &ColumnDef) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {}",
        quote_ident(table_name),
        render_column_definition(column)
    )
}

fn render_copy_sql(
    from_table: &str,
    to_table: &str,
    retained_columns: &[(String, String)],
) -> String {
    let to_columns = retained_columns
        .iter()
        .map(|(_, to_column)| quote_ident(to_column))
        .collect::<Vec<_>>()
        .join(", ");
    let from_columns = retained_columns
        .iter()
        .map(|(from_column, _)| quote_ident(from_column))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {} ({}) SELECT {} FROM {}",
        quote_ident(to_table),
        to_columns,
        from_columns,
        quote_ident(from_table)
    )
}

fn render_column_definition(column: &ColumnDef) -> String {
    if column.nullable {
        format!("{} {}", quote_ident(&column.name), column.sql_type)
    } else {
        format!("{} {} NOT NULL", quote_ident(&column.name), column.sql_type)
    }
}

fn quote_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn format_blockers(blockers: &[super::plan::PlanBlocker]) -> String {
    blockers
        .iter()
        .map(|blocker| match blocker {
            super::plan::PlanBlocker::RegistryUncertain(message)
            | super::plan::PlanBlocker::Uncertain(message) => message.clone(),
            super::plan::PlanBlocker::RequiredColumnAddition { table, column } => format!(
                "table `{table}` needs required column `{column}`, which cannot be added safely"
            ),
            super::plan::PlanBlocker::UnsupportedInlineFeature { table, feature } => {
                format!("table `{table}` uses unsupported feature: {feature}")
            }
            super::plan::PlanBlocker::RealForeignKeys { table } => {
                format!("table `{table}` participates in real foreign keys")
            }
            super::plan::PlanBlocker::DependentView { table, view } => {
                format!("table `{table}` is referenced by dependent view `{view}`")
            }
            super::plan::PlanBlocker::DependentTrigger { table, trigger } => {
                format!("table `{table}` is referenced by dependent trigger `{trigger}`")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}
