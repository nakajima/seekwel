use rusqlite::OptionalExtension;

use crate::connection::{record_query, record_query_with_params};
use crate::error::Error;

use super::types::{ARTIFACT_VERSION, HISTORY_TABLE, SchemaDef};

pub(crate) fn load_latest_target_schema(
    conn: &rusqlite::Connection,
) -> Result<Option<SchemaDef>, Error> {
    if !history_table_exists(conn)? {
        return Ok(None);
    }

    let query = format!(
        "SELECT target_schema FROM {HISTORY_TABLE} ORDER BY id DESC LIMIT 1"
    );
    record_query(&query);
    let schema = conn
        .query_row(
            &query,
            (),
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Error::Sqlite)?;

    schema
        .as_deref()
        .map(SchemaDef::from_canonical)
        .transpose()
}

pub(crate) fn ensure_history_table(tx: &rusqlite::Transaction<'_>) -> Result<(), Error> {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {HISTORY_TABLE} (\
         id INTEGER PRIMARY KEY, \
         plan_id TEXT NOT NULL, \
         artifact_version INTEGER NOT NULL, \
         source_fingerprint TEXT NOT NULL, \
         target_fingerprint TEXT NOT NULL, \
         target_schema TEXT NOT NULL, \
         created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)"
    );
    record_query(&sql);
    tx.execute_batch(&sql)
    .map_err(Error::Sqlite)
}

pub(crate) fn record_success(
    tx: &rusqlite::Transaction<'_>,
    plan_id: &str,
    source_fingerprint: &str,
    target_fingerprint: &str,
    target_schema: &SchemaDef,
) -> Result<(), Error> {
    ensure_history_table(tx)?;
    let sql = format!(
        "INSERT INTO {HISTORY_TABLE} \
         (plan_id, artifact_version, source_fingerprint, target_fingerprint, target_schema) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
    );
    record_query_with_params(
        &sql,
        &[
            rusqlite::types::Value::Text(plan_id.to_string()),
            rusqlite::types::Value::Integer(i64::from(ARTIFACT_VERSION)),
            rusqlite::types::Value::Text(source_fingerprint.to_string()),
            rusqlite::types::Value::Text(target_fingerprint.to_string()),
            rusqlite::types::Value::Text(target_schema.canonical_string()),
        ],
    );
    tx.execute(
        &sql,
        (
            plan_id,
            i64::from(ARTIFACT_VERSION),
            source_fingerprint,
            target_fingerprint,
            target_schema.canonical_string(),
        ),
    )
    .map_err(Error::Sqlite)?;
    Ok(())
}

fn history_table_exists(conn: &rusqlite::Connection) -> Result<bool, Error> {
    let query = "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)";
    record_query_with_params(query, &[rusqlite::types::Value::Text(HISTORY_TABLE.to_string())]);
    conn.query_row(
        query,
        [HISTORY_TABLE],
        |row| row.get::<_, i64>(0),
    )
    .map(|value| value != 0)
    .map_err(Error::Sqlite)
}
