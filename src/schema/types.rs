use std::fmt::{self, Write};

use crate::error::Error;

pub(crate) const ARTIFACT_VERSION: u32 = 1;
pub(crate) const HISTORY_TABLE: &str = "_seekwel_schema_history";

/// A normalized schema snapshot for seekwel-managed tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaDef {
    /// Managed tables in deterministic name order.
    pub tables: Vec<TableDef>,
}

impl SchemaDef {
    pub(crate) fn normalized(mut self) -> Result<Self, Error> {
        self.tables.sort_by(|a, b| a.name.cmp(&b.name));
        for table in &mut self.tables {
            table.columns.sort_by(|a, b| a.name.cmp(&b.name));
        }

        for pair in self.tables.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(Error::InvalidSchema(format!(
                    "duplicate managed table `{}`",
                    pair[0].name
                )));
            }
        }

        for table in &self.tables {
            for pair in table.columns.windows(2) {
                if pair[0].name == pair[1].name {
                    return Err(Error::InvalidSchema(format!(
                        "duplicate column `{}` in table `{}`",
                        pair[0].name, table.name
                    )));
                }
            }
            if table.columns.iter().any(|column| column.name == table.primary_key.name) {
                return Err(Error::InvalidSchema(format!(
                    "table `{}` duplicates primary key column `{}` in managed columns",
                    table.name, table.primary_key.name
                )));
            }
        }

        Ok(self)
    }

    pub(crate) fn canonical_string(&self) -> String {
        let mut out = String::new();
        for table in &self.tables {
            let _ = writeln!(
                &mut out,
                "table\t{}\t{}\t{}",
                table.name, table.primary_key.name, table.primary_key.sql_type
            );
            for column in &table.columns {
                let _ = writeln!(
                    &mut out,
                    "column\t{}\t{}\t{}\t{}",
                    table.name,
                    column.name,
                    column.sql_type,
                    u8::from(column.nullable)
                );
            }
        }
        out
    }

    pub(crate) fn from_canonical(input: &str) -> Result<Self, Error> {
        let mut tables = Vec::<TableDef>::new();

        for line in input.lines() {
            if line.is_empty() {
                continue;
            }

            let mut parts = line.split('\t');
            let Some(kind) = parts.next() else {
                continue;
            };

            match kind {
                "table" => {
                    let name = parts
                        .next()
                        .ok_or_else(|| Error::InvalidSchema("history row is missing table name".into()))?;
                    let primary_key_name = parts.next().unwrap_or("id");
                    let primary_key_sql_type = parts.next().unwrap_or("INTEGER");
                    if parts.next().is_some() {
                        return Err(Error::InvalidSchema(format!(
                            "history table row for `{name}` has trailing data"
                        )));
                    }
                    tables.push(TableDef {
                        name: name.to_string(),
                        primary_key: PrimaryKeyDef {
                            name: primary_key_name.to_string(),
                            sql_type: primary_key_sql_type.to_string(),
                        },
                        columns: Vec::new(),
                    });
                }
                "column" => {
                    let table_name = parts.next().ok_or_else(|| {
                        Error::InvalidSchema("history row is missing column table name".into())
                    })?;
                    let column_name = parts.next().ok_or_else(|| {
                        Error::InvalidSchema("history row is missing column name".into())
                    })?;
                    let sql_type = parts.next().ok_or_else(|| {
                        Error::InvalidSchema("history row is missing column type".into())
                    })?;
                    let nullable = parts.next().ok_or_else(|| {
                        Error::InvalidSchema("history row is missing column nullability".into())
                    })?;
                    if parts.next().is_some() {
                        return Err(Error::InvalidSchema(format!(
                            "history column row for `{table_name}.{column_name}` has trailing data"
                        )));
                    }

                    let Some(table) = tables.iter_mut().find(|table| table.name == table_name) else {
                        return Err(Error::InvalidSchema(format!(
                            "history column row references unknown table `{table_name}`"
                        )));
                    };

                    table.columns.push(ColumnDef {
                        name: column_name.to_string(),
                        sql_type: sql_type.to_string(),
                        nullable: match nullable {
                            "0" => false,
                            "1" => true,
                            other => {
                                return Err(Error::InvalidSchema(format!(
                                    "history column `{table_name}.{column_name}` has invalid nullability flag `{other}`"
                                )));
                            }
                        },
                    });
                }
                other => {
                    return Err(Error::InvalidSchema(format!(
                        "history row has unknown record kind `{other}`"
                    )));
                }
            }
        }

        SchemaDef { tables }.normalized()
    }

    /// Returns a stable fingerprint for the normalized schema.
    pub fn fingerprint(&self) -> String {
        let canonical = self.canonical_string();
        format!("{:016x}", fnv1a64(canonical.as_bytes()))
    }

    pub(crate) fn table_names(&self) -> Vec<String> {
        self.tables.iter().map(|table| table.name.clone()).collect()
    }
}

/// A managed SQLite table definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDef {
    /// The SQLite table name.
    pub name: String,
    /// The SQLite primary-key column.
    pub primary_key: PrimaryKeyDef,
    /// All managed non-primary-key columns in deterministic name order.
    pub columns: Vec<ColumnDef>,
}

/// A managed SQLite primary-key definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryKeyDef {
    /// The SQLite primary-key column name.
    pub name: String,
    /// The declared SQLite type used for DDL and review output.
    pub sql_type: String,
}

impl PrimaryKeyDef {
    pub(crate) fn affinity(&self) -> SqlAffinity {
        SqlAffinity::from_declared_type(&self.sql_type)
    }
}

/// A managed non-primary-key SQLite column definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    /// The SQLite column name.
    pub name: String,
    /// The declared SQLite type used for DDL and review output.
    pub sql_type: String,
    /// Whether the column may store `NULL`.
    pub nullable: bool,
}

impl ColumnDef {
    pub(crate) fn affinity(&self) -> SqlAffinity {
        SqlAffinity::from_declared_type(&self.sql_type)
    }
}

/// SQLite type affinity used for supported column comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlAffinity {
    Integer,
    Real,
    Text,
    Blob,
    Numeric,
}

impl SqlAffinity {
    pub(crate) fn from_declared_type(declared: &str) -> Self {
        let upper = declared.trim().to_ascii_uppercase();
        if upper.contains("INT") {
            Self::Integer
        } else if upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT") {
            Self::Text
        } else if upper.contains("BLOB") || upper.is_empty() {
            Self::Blob
        } else if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
            Self::Real
        } else {
            Self::Numeric
        }
    }
}

pub(crate) fn stable_hash_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

pub(crate) fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(&mut escaped, "\\u{:04x}", c as u32);
            }
            c => escaped.push(c),
        }
    }
    escaped
}

impl fmt::Display for SchemaDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.canonical_string())
    }
}
