use std::sync::{Mutex, OnceLock};

use seekwel::ModelRecord;
use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    age: Option<u8>,
}

fn expect_new_record(_: &Person<seekwel::NewRecord>) {}
fn expect_persisted(_: &Person) {}

static MODEL_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_person_table(test: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    let _guard = MODEL_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(error) => return Err(error),
    }

    Person::create_table()?;
    Connection::get()?.execute("DELETE FROM person", ())?;

    test()
}

#[test]
fn builder_create_or_update_by_creates_and_updates() -> Result<(), Error> {
    with_person_table(|| {
        let pat = Person::builder()
            .name("Pat")
            .age(Some(20))
            .create_or_update_by([PersonColumns::Name])?;
        assert_eq!(pat.id, 1);
        assert_eq!(pat.name, "Pat");
        assert_eq!(pat.age, Some(20));

        let updated = Person::builder()
            .name("Pat")
            .age(Some(21))
            .create_or_update_by([PersonColumns::Name])?;
        assert_eq!(updated.id, pat.id);
        assert_eq!(updated.age, Some(21));
        assert_eq!(Person::count()?, 1);

        let unchanged_optional = Person::builder()
            .name("Pat")
            .create_or_update_by([PersonColumns::Name])?;
        assert_eq!(unchanged_optional.id, pat.id);
        assert_eq!(unchanged_optional.age, Some(21));

        Ok(())
    })
}

#[test]
fn builder_create_or_update_by_rejects_empty_lookup() -> Result<(), Error> {
    with_person_table(|| {
        let result = Person::builder().name("Pat").create_or_update_by([]);
        assert!(matches!(
            result,
            Err(seekwel::CreateOrUpdateError::Error(Error::InvalidQuery(_)))
        ));

        Ok(())
    })
}

#[test]
fn builder_save_reload_and_delete() -> Result<(), Error> {
    with_person_table(|| {
        let draft = Person::builder().name("Pat").age(Some(100)).build()?;
        expect_new_record(&draft);
        assert_eq!(draft.name, "Pat");
        assert_eq!(draft.age, Some(100));
        assert_eq!(draft.id, 0);
        assert_eq!(draft.persisted_id(), None);
        assert_eq!(draft.persisted_primary_key_value(), None);
        assert!(draft.is_new_record());

        let mut person = draft.save()?;
        expect_persisted(&person);
        assert_eq!(person.id, 1);
        assert_eq!(person.persisted_id(), Some(1));
        assert_eq!(
            person.persisted_primary_key_value(),
            Some(rusqlite::types::Value::Integer(1))
        );
        assert!(person.is_persisted());

        let id = person.id;
        person.reload()?;
        assert_eq!(person.id, id);
        assert_eq!(person.name, "Pat");
        assert_eq!(person.age, Some(100));

        person.name = "Patricia".to_string();
        person.age = None;
        person.save()?;

        let refreshed = Person::find(person.id)?;
        assert_eq!(refreshed.name, "Patricia");
        assert_eq!(refreshed.age, None);

        person.reload()?;
        assert_eq!(person.name, "Patricia");
        assert_eq!(person.age, None);

        let person2 = Person::builder().name("Sam").create()?;
        assert_eq!(person2.name, "Sam");
        assert_eq!(person2.age, None);
        assert_eq!(person2.id, 2);

        let deleted_id = person.id;
        let remaining_id = person2.id;
        person.delete()?;

        assert!(matches!(Person::find(deleted_id), Err(Error::Sqlite(_))));

        let remaining = Person::all()?;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, remaining_id);
        assert_eq!(remaining[0].name, "Sam");
        assert_eq!(remaining[0].age, None);

        Ok(())
    })
}
