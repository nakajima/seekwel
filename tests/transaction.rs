use std::panic::{self, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};

use seekwel::Comparison;
use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
}

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn setup() -> Result<std::sync::MutexGuard<'static, ()>, Error> {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(error) => return Err(error),
    }
    Person::create_table()?;
    Connection::get()?.execute("DELETE FROM person", ())?;
    Ok(lock)
}

#[test]
fn transaction_commits_implicit_model_calls() -> Result<(), Error> {
    let _lock = setup()?;

    Connection::transaction(|| {
        Person::builder().name("Pat").create()?;
        assert_eq!(Person::count()?, 1);
        Ok(())
    })?;

    assert_eq!(Person::count()?, 1);
    Ok(())
}

#[test]
fn transaction_rolls_back_on_error() -> Result<(), Error> {
    let _lock = setup()?;

    let result = Connection::transaction(|| {
        Person::builder().name("Pat").create()?;
        Connection::rollback::<()>()
    });

    assert!(matches!(result, Err(Error::Rollback)));
    assert_eq!(Person::count()?, 0);
    Ok(())
}

#[test]
fn transaction_rolls_back_on_panic() -> Result<(), Error> {
    let _lock = setup()?;

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let _ = Connection::transaction(|| -> Result<(), Error> {
            Person::builder().name("Pat").create()?;
            panic!("boom");
        });
    }));

    assert!(result.is_err());
    assert_eq!(Person::count()?, 0);
    Person::builder().name("After").create()?;
    assert_eq!(Person::count()?, 1);
    Ok(())
}

#[test]
fn nested_transaction_rolls_back_to_savepoint() -> Result<(), Error> {
    let _lock = setup()?;

    Connection::transaction(|| {
        Person::builder().name("Outer").create()?;

        let result = Connection::transaction(|| {
            Person::builder().name("Inner").create()?;
            Connection::rollback::<()>()
        });
        assert!(matches!(result, Err(Error::Rollback)));

        assert_eq!(Person::count()?, 1);
        Ok(())
    })?;

    let people = Person::q(PersonColumns::Name, Comparison::Eq("Outer")).all()?;
    assert_eq!(people.len(), 1);
    assert_eq!(Person::count()?, 1);
    Ok(())
}
