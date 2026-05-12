use seekwel::Error;
use seekwel::connection::Connection;

#[test]
fn get_before_initialization_returns_not_initialized() {
    assert!(matches!(Connection::get(), Err(Error::NotInitialized)));
}

#[test]
fn transaction_before_initialization_returns_not_initialized() {
    let result: Result<(), Error> = Connection::transaction(|| Ok(()));
    assert!(matches!(result, Err(Error::NotInitialized)));
}
