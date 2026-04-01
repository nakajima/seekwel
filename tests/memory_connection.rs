use seekwel::connection::Connection;
use seekwel::error::Error;

#[test]
fn initialize_and_get() -> Result<(), Error> {
    Connection::memory()?;
    let conn = Connection::get()?;
    conn.execute("CREATE TABLE people (id INTEGER PRIMARY KEY)", ())?;
    Ok(())
}
