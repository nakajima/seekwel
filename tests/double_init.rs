use seekwel::connection::Connection;
use seekwel::error::Error;

#[test]
fn double_init_returns_error() -> Result<(), Error> {
    Connection::memory()?;
    let err = Connection::memory().unwrap_err();
    assert!(matches!(err, Error::AlreadyInitialized));
    Ok(())
}
