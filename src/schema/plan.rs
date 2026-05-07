use crate::connection::Connection;
use crate::error::Error;

use super::apply;
use super::diff;
use super::introspect;
use super::types::{json_escape, stable_hash_hex, ARTIFACT_VERSION, ColumnDef, SchemaDef, TableDef};

/// Controls which schema plans may be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    /// Apply only plans that do not contain destructive operations.
    SafeOnly,
    /// Allow both safe and destructive operations, as long as no blockers remain.
    AllowDestructive,
}

/// A reviewable schema reconciliation plan.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Artifact version for deterministic plan serialization.
    pub artifact_version: u32,
    /// The normalized managed source schema read from the live database.
    pub source: SchemaDef,
    /// The normalized managed target schema derived from model metadata.
    pub target: SchemaDef,
    /// Fingerprint of the source schema.
    pub source_fingerprint: String,
    /// Fingerprint of the target schema.
    pub target_fingerprint: String,
    /// Planned schema operations in deterministic order.
    pub ops: Vec<PlanOp>,
    /// Conditions that prevent automatic apply.
    pub blockers: Vec<PlanBlocker>,
    /// Deterministic plan identity.
    pub plan_id: String,
}

impl Plan {
    pub(crate) fn build(target: SchemaDef) -> Result<Self, Error> {
        let conn = Connection::get()?;
        let scope = introspect::managed_scope(conn.raw(), &target)?;
        let actual = introspect::introspect_managed(conn.raw(), &scope)?;
        let diff = diff::diff(&target, &actual)?;
        let source_fingerprint = diff.source.fingerprint();
        let target_fingerprint = target.fingerprint();

        let mut plan = Self {
            artifact_version: ARTIFACT_VERSION,
            source: diff.source,
            target,
            source_fingerprint,
            target_fingerprint,
            ops: diff.ops,
            blockers: diff.blockers,
            plan_id: String::new(),
        };
        plan.plan_id = plan.compute_plan_id();
        Ok(plan)
    }

    /// Returns `true` when automatic apply must refuse the plan.
    pub fn is_blocked(&self) -> bool {
        !self.blockers.is_empty()
    }

    /// Returns `true` when the plan contains any destructive operation.
    pub fn is_destructive(&self) -> bool {
        self.ops.iter().any(PlanOp::is_destructive)
    }

    /// Renders the plan as deterministic JSON for review or storage.
    pub fn to_json_string(&self) -> String {
        let source_tables = schema_tables_json(&self.source);
        let target_tables = schema_tables_json(&self.target);
        let ops = self
            .ops
            .iter()
            .map(PlanOp::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let blockers = self
            .blockers
            .iter()
            .map(PlanBlocker::to_json)
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"artifact_version\":{},\"plan_id\":\"{}\",\"source_fingerprint\":\"{}\",\"target_fingerprint\":\"{}\",\"source\":{{\"tables\":[{}]}},\"target\":{{\"tables\":[{}]}},\"ops\":[{}],\"blockers\":[{}]}}",
            self.artifact_version,
            json_escape(&self.plan_id),
            json_escape(&self.source_fingerprint),
            json_escape(&self.target_fingerprint),
            source_tables,
            target_tables,
            ops,
            blockers,
        )
    }

    /// Applies the plan against the current live database.
    pub fn apply(&self, mode: ApplyMode) -> Result<(), Error> {
        apply::apply(self, mode)
    }

    fn compute_plan_id(&self) -> String {
        let mut data = String::new();
        data.push_str(&format!("artifact_version={}\n", self.artifact_version));
        data.push_str("source\n");
        data.push_str(&self.source.canonical_string());
        data.push_str("target\n");
        data.push_str(&self.target.canonical_string());
        for op in &self.ops {
            data.push_str(&format!("op={:?}\n", op));
        }
        for blocker in &self.blockers {
            data.push_str(&format!("blocker={:?}\n", blocker));
        }
        stable_hash_hex(data.as_bytes())
    }
}

/// A planned schema operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanOp {
    CreateTable { table: TableDef },
    AddColumn { table: String, column: ColumnDef },
    RebuildTable {
        table: String,
        from: TableDef,
        to: TableDef,
        reasons: Vec<RebuildReason>,
    },
    DropTable { table: TableDef },
}

impl PlanOp {
    pub fn is_destructive(&self) -> bool {
        matches!(self, Self::RebuildTable { .. } | Self::DropTable { .. })
    }

    fn to_json(&self) -> String {
        match self {
            Self::CreateTable { table } => format!(
                "{{\"kind\":\"create_table\",\"table\":{}}}",
                table_json(table)
            ),
            Self::AddColumn { table, column } => format!(
                "{{\"kind\":\"add_column\",\"table\":\"{}\",\"column\":{}}}",
                json_escape(table),
                column_json(column)
            ),
            Self::RebuildTable { table, reasons, .. } => format!(
                "{{\"kind\":\"rebuild_table\",\"table\":\"{}\",\"reasons\":[{}]}}",
                json_escape(table),
                reasons
                    .iter()
                    .map(RebuildReason::to_json)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::DropTable { table } => format!(
                "{{\"kind\":\"drop_table\",\"table\":{}}}",
                table_json(table)
            ),
        }
    }
}

/// Why a table rebuild is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    DropColumns { columns: Vec<String> },
    ChangePrimaryKeyName {
        from: String,
        to: String,
    },
    ChangePrimaryKeyType {
        from: String,
        to: String,
    },
    ChangeColumnType {
        column: String,
        from: String,
        to: String,
    },
    ChangeNullability {
        column: String,
        from_nullable: bool,
        to_nullable: bool,
    },
}

impl RebuildReason {
    fn to_json(&self) -> String {
        match self {
            Self::DropColumns { columns } => format!(
                "{{\"kind\":\"drop_columns\",\"columns\":[{}]}}",
                columns
                    .iter()
                    .map(|column| format!("\"{}\"", json_escape(column)))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::ChangePrimaryKeyName { from, to } => format!(
                "{{\"kind\":\"change_primary_key_name\",\"from\":\"{}\",\"to\":\"{}\"}}",
                json_escape(from),
                json_escape(to)
            ),
            Self::ChangePrimaryKeyType { from, to } => format!(
                "{{\"kind\":\"change_primary_key_type\",\"from\":\"{}\",\"to\":\"{}\"}}",
                json_escape(from),
                json_escape(to)
            ),
            Self::ChangeColumnType { column, from, to } => format!(
                "{{\"kind\":\"change_column_type\",\"column\":\"{}\",\"from\":\"{}\",\"to\":\"{}\"}}",
                json_escape(column),
                json_escape(from),
                json_escape(to)
            ),
            Self::ChangeNullability {
                column,
                from_nullable,
                to_nullable,
            } => format!(
                "{{\"kind\":\"change_nullability\",\"column\":\"{}\",\"from_nullable\":{},\"to_nullable\":{}}}",
                json_escape(column),
                from_nullable,
                to_nullable
            ),
        }
    }
}

/// A condition that prevents automatic apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanBlocker {
    RegistryUncertain(String),
    RequiredColumnAddition { table: String, column: String },
    UnsupportedInlineFeature { table: String, feature: String },
    RealForeignKeys { table: String },
    DependentView { table: String, view: String },
    DependentTrigger { table: String, trigger: String },
    Uncertain(String),
}

impl PlanBlocker {
    fn to_json(&self) -> String {
        match self {
            Self::RegistryUncertain(message) => format!(
                "{{\"kind\":\"registry_uncertain\",\"message\":\"{}\"}}",
                json_escape(message)
            ),
            Self::RequiredColumnAddition { table, column } => format!(
                "{{\"kind\":\"required_column_addition\",\"table\":\"{}\",\"column\":\"{}\"}}",
                json_escape(table),
                json_escape(column)
            ),
            Self::UnsupportedInlineFeature { table, feature } => format!(
                "{{\"kind\":\"unsupported_inline_feature\",\"table\":\"{}\",\"feature\":\"{}\"}}",
                json_escape(table),
                json_escape(feature)
            ),
            Self::RealForeignKeys { table } => format!(
                "{{\"kind\":\"real_foreign_keys\",\"table\":\"{}\"}}",
                json_escape(table)
            ),
            Self::DependentView { table, view } => format!(
                "{{\"kind\":\"dependent_view\",\"table\":\"{}\",\"view\":\"{}\"}}",
                json_escape(table),
                json_escape(view)
            ),
            Self::DependentTrigger { table, trigger } => format!(
                "{{\"kind\":\"dependent_trigger\",\"table\":\"{}\",\"trigger\":\"{}\"}}",
                json_escape(table),
                json_escape(trigger)
            ),
            Self::Uncertain(message) => format!(
                "{{\"kind\":\"uncertain\",\"message\":\"{}\"}}",
                json_escape(message)
            ),
        }
    }
}

fn schema_tables_json(schema: &SchemaDef) -> String {
    schema.tables.iter().map(table_json).collect::<Vec<_>>().join(",")
}

fn table_json(table: &TableDef) -> String {
    format!(
        "{{\"name\":\"{}\",\"primary_key\":{{\"name\":\"{}\",\"sql_type\":\"{}\"}},\"columns\":[{}]}}",
        json_escape(&table.name),
        json_escape(&table.primary_key.name),
        json_escape(&table.primary_key.sql_type),
        table.columns.iter().map(column_json).collect::<Vec<_>>().join(",")
    )
}

fn column_json(column: &ColumnDef) -> String {
    format!(
        "{{\"name\":\"{}\",\"sql_type\":\"{}\",\"nullable\":{}}}",
        json_escape(&column.name),
        json_escape(&column.sql_type),
        column.nullable
    )
}
