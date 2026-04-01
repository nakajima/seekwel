use std::time::SystemTime;

use seekwel::connection::Connection;
use seekwel::error::Error;

#[test]
fn initialize_and_get() -> Result<(), Error> {
    let path = format!("/tmp/seekwel-test-{:?}.sqlite", SystemTime::now());
    Connection::file(&path)?;
    let conn = Connection::get()?;
    conn.execute("CREATE TABLE people (id INTEGER PRIMARY KEY)", ())?;
    Ok(())
}
