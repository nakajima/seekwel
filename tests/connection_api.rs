use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use seekwel::Error;
use seekwel::connection::Connection;

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static DB_PATH: OnceLock<PathBuf> = OnceLock::new();

fn setup() -> Result<std::sync::MutexGuard<'static, ()>, Error> {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let path = DB_PATH.get_or_init(|| {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("seekwel-connection-api-{suffix}.sqlite"))
    });

    match Connection::file(&path.to_string_lossy()) {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(error) => return Err(error),
    }

    let conn = Connection::get()?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS raw_things (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        (),
    )?;
    conn.execute("DELETE FROM raw_things", ())?;
    Ok(lock)
}

#[test]
fn execute_insert_and_query_methods_work_on_file_database() -> Result<(), Error> {
    let _lock = setup()?;
    let conn = Connection::get()?;

    let id = conn.insert("INSERT INTO raw_things (name) VALUES (?1)", ["Pat"])?;
    let name = conn.query_row(
        "SELECT name FROM raw_things WHERE id = ?1",
        [id as i64],
        |row| row.get::<_, String>(0),
    )?;
    assert_eq!(name, "Pat");

    let found = conn.query_optional(
        "SELECT name FROM raw_things WHERE name = ?1",
        ["Pat"],
        |row| row.get::<_, String>(0),
    )?;
    assert_eq!(found.as_deref(), Some("Pat"));

    let missing = conn.query_optional(
        "SELECT name FROM raw_things WHERE name = ?1",
        ["Missing"],
        |row| row.get::<_, String>(0),
    )?;
    assert_eq!(missing, None);

    let names = conn.query_all("SELECT name FROM raw_things ORDER BY id", (), |row| {
        row.get::<_, String>(0)
    })?;
    assert_eq!(names, vec!["Pat"]);

    Ok(())
}

#[test]
fn recent_queries_reports_seekwel_managed_sql() -> Result<(), Error> {
    let _lock = setup()?;

    Connection::transaction(|| Ok(()))?;
    let recent = Connection::recent_queries();

    assert!(recent.iter().any(|query| query == "BEGIN IMMEDIATE"));
    assert!(recent.iter().any(|query| query == "COMMIT"));
    Ok(())
}

#[test]
fn public_query_row_can_execute_write_returning_sql_on_file_database() -> Result<(), Error> {
    let _lock = setup()?;
    let conn = Connection::get()?;

    let name = conn.query_row(
        "INSERT INTO raw_things (name) VALUES (?1) RETURNING name",
        ["Returned"],
        |row| row.get::<_, String>(0),
    )?;

    assert_eq!(name, "Returned");
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM raw_things", (), |row| {
            row.get::<_, i64>(0)
        })?,
        1
    );

    Ok(())
}
