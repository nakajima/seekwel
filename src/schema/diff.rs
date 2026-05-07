use std::collections::{BTreeMap, BTreeSet};

use crate::error::Error;

use super::introspect::ActualTable;
use super::plan::{PlanBlocker, PlanOp, RebuildReason};
use super::types::{ColumnDef, SchemaDef};

pub(crate) struct DiffResult {
    pub(crate) source: SchemaDef,
    pub(crate) ops: Vec<PlanOp>,
    pub(crate) blockers: Vec<PlanBlocker>,
}

pub(crate) fn diff(
    target: &SchemaDef,
    actual: &BTreeMap<String, ActualTable>,
) -> Result<DiffResult, Error> {
    let source = SchemaDef {
        tables: actual.values().map(|table| table.table.clone()).collect(),
    }
    .normalized()?;

    let target_by_name: BTreeMap<_, _> = target
        .tables
        .iter()
        .map(|table| (table.name.as_str(), table))
        .collect();

    let mut ops = Vec::new();
    let mut blockers = Vec::new();

    for (table_name, desired) in &target_by_name {
        let Some(actual) = actual.get(*table_name) else {
            ops.push(PlanOp::CreateTable {
                table: (*desired).clone(),
            });
            continue;
        };

        let mut desired_columns: BTreeMap<&str, &ColumnDef> = BTreeMap::new();
        for column in &desired.columns {
            desired_columns.insert(column.name.as_str(), column);
        }

        let mut actual_columns: BTreeMap<&str, &ColumnDef> = BTreeMap::new();
        for column in &actual.table.columns {
            actual_columns.insert(column.name.as_str(), column);
        }

        let mut add_columns = Vec::new();
        let mut rebuild_reasons = Vec::new();
        let mut removed_columns = Vec::new();

        if desired.primary_key.name != actual.table.primary_key.name {
            rebuild_reasons.push(RebuildReason::ChangePrimaryKeyName {
                from: actual.table.primary_key.name.clone(),
                to: desired.primary_key.name.clone(),
            });
        }
        if desired.primary_key.affinity() != actual.table.primary_key.affinity()
            || desired.primary_key.sql_type != actual.table.primary_key.sql_type
        {
            rebuild_reasons.push(RebuildReason::ChangePrimaryKeyType {
                from: actual.table.primary_key.sql_type.clone(),
                to: desired.primary_key.sql_type.clone(),
            });
        }

        for (name, desired_column) in &desired_columns {
            match actual_columns.get(name) {
                None => {
                    if desired_column.nullable {
                        add_columns.push((*desired_column).clone());
                    } else {
                        blockers.push(PlanBlocker::RequiredColumnAddition {
                            table: (*table_name).to_string(),
                            column: (*name).to_string(),
                        });
                    }
                }
                Some(actual_column) => {
                    if desired_column.affinity() != actual_column.affinity() {
                        rebuild_reasons.push(RebuildReason::ChangeColumnType {
                            column: (*name).to_string(),
                            from: actual_column.sql_type.clone(),
                            to: desired_column.sql_type.clone(),
                        });
                    }
                    if desired_column.nullable != actual_column.nullable {
                        rebuild_reasons.push(RebuildReason::ChangeNullability {
                            column: (*name).to_string(),
                            from_nullable: actual_column.nullable,
                            to_nullable: desired_column.nullable,
                        });
                    }
                }
            }
        }

        for name in actual_columns.keys() {
            if !desired_columns.contains_key(name) {
                removed_columns.push((*name).to_string());
            }
        }
        if !removed_columns.is_empty() {
            rebuild_reasons.push(RebuildReason::DropColumns {
                columns: removed_columns,
            });
        }

        if !rebuild_reasons.is_empty() {
            for feature in &actual.unsupported_inline_features {
                blockers.push(PlanBlocker::UnsupportedInlineFeature {
                    table: (*table_name).to_string(),
                    feature: feature.clone(),
                });
            }
            if actual.has_real_foreign_keys {
                blockers.push(PlanBlocker::RealForeignKeys {
                    table: (*table_name).to_string(),
                });
            }
            for name in &actual.dependent_views {
                blockers.push(PlanBlocker::DependentView {
                    table: (*table_name).to_string(),
                    view: name.clone(),
                });
            }
            for name in &actual.dependent_external_triggers {
                blockers.push(PlanBlocker::DependentTrigger {
                    table: (*table_name).to_string(),
                    trigger: name.clone(),
                });
            }

            ops.push(PlanOp::RebuildTable {
                table: (*table_name).to_string(),
                from: actual.table.clone(),
                to: (*desired).clone(),
                reasons: rebuild_reasons,
            });
            continue;
        }

        add_columns.sort_by(|a, b| a.name.cmp(&b.name));
        for column in add_columns {
            ops.push(PlanOp::AddColumn {
                table: (*table_name).to_string(),
                column,
            });
        }
    }

    let target_names: BTreeSet<_> = target_by_name.keys().copied().collect();
    for (table_name, actual_table) in actual {
        if target_names.contains(table_name.as_str()) {
            continue;
        }

        if actual_table.has_real_foreign_keys {
            blockers.push(PlanBlocker::RealForeignKeys {
                table: table_name.clone(),
            });
        }
        for view in &actual_table.dependent_views {
            blockers.push(PlanBlocker::DependentView {
                table: table_name.clone(),
                view: view.clone(),
            });
        }
        for trigger in &actual_table.dependent_external_triggers {
            blockers.push(PlanBlocker::DependentTrigger {
                table: table_name.clone(),
                trigger: trigger.clone(),
            });
        }

        ops.push(PlanOp::DropTable {
            table: actual_table.table.clone(),
        });
    }

    ops.sort_by(|left, right| plan_op_sort_key(left).cmp(&plan_op_sort_key(right)));
    blockers.sort_by(|left, right| blocker_sort_key(left).cmp(&blocker_sort_key(right)));

    Ok(DiffResult {
        source,
        ops,
        blockers,
    })
}

fn plan_op_sort_key(op: &PlanOp) -> (u8, &str, &str) {
    match op {
        PlanOp::CreateTable { table } => (0, table.name.as_str(), ""),
        PlanOp::AddColumn { table, column } => (1, table.as_str(), column.name.as_str()),
        PlanOp::RebuildTable { table, .. } => (2, table.as_str(), ""),
        PlanOp::DropTable { table } => (3, table.name.as_str(), ""),
    }
}

fn blocker_sort_key(blocker: &PlanBlocker) -> (u8, &str, &str) {
    match blocker {
        PlanBlocker::RegistryUncertain(message) => (0, message.as_str(), ""),
        PlanBlocker::RequiredColumnAddition { table, column } => (1, table.as_str(), column.as_str()),
        PlanBlocker::UnsupportedInlineFeature { table, feature } => {
            (2, table.as_str(), feature.as_str())
        }
        PlanBlocker::RealForeignKeys { table } => (3, table.as_str(), ""),
        PlanBlocker::DependentView { table, view } => (4, table.as_str(), view.as_str()),
        PlanBlocker::DependentTrigger { table, trigger } => {
            (5, table.as_str(), trigger.as_str())
        }
        PlanBlocker::Uncertain(message) => (6, message.as_str(), ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::introspect::ActualTable;
    use crate::schema::types::{ColumnDef, PrimaryKeyDef, TableDef};

    fn empty_actual(table: TableDef) -> ActualTable {
        ActualTable {
            table,
            unsupported_inline_features: Vec::new(),
            has_real_foreign_keys: false,
            dependent_views: Vec::new(),
            dependent_external_triggers: Vec::new(),
            replay_sql: Vec::new(),
        }
    }

    #[test]
    fn diff_propagates_invalid_actual_schema() {
        let mut actual = BTreeMap::new();
        actual.insert(
            "thing".to_string(),
            empty_actual(TableDef {
                name: "thing".to_string(),
                primary_key: PrimaryKeyDef {
                    name: "id".to_string(),
                    sql_type: "INTEGER".to_string(),
                },
                columns: vec![
                    ColumnDef {
                        name: "label".to_string(),
                        sql_type: "TEXT".to_string(),
                        nullable: true,
                    },
                    ColumnDef {
                        name: "label".to_string(),
                        sql_type: "TEXT".to_string(),
                        nullable: true,
                    },
                ],
            }),
        );

        let target = SchemaDef::default();
        let result = diff(&target, &actual);

        match result {
            Err(Error::InvalidSchema(message)) => {
                assert!(
                    message.contains("duplicate column"),
                    "expected duplicate-column error, got: {message}"
                );
            }
            Err(other) => panic!("expected InvalidSchema error, got: {other}"),
            Ok(_) => panic!("expected InvalidSchema error, got Ok(_)"),
        }
    }
}
