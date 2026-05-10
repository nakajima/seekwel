use std::sync::{Mutex, OnceLock};

use seekwel::ModelRecord;
use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

struct PersonValidator;

#[seekwel::model(validator = PersonValidator)]
struct Person {
    id: u64,
    name: String,
}

impl<S> seekwel::Validator<Person<S>> for PersonValidator {
    fn validate(person: &Person<S>, errors: &mut seekwel::Errors<PersonColumns>) {
        if person.name.trim().is_empty() {
            errors.add(PersonColumns::Name, "can't be blank");
        }
    }
}

static VALIDATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_validation_schema(test: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    let _guard = VALIDATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(err) => return Err(err),
    }

    Person::create_table()?;
    Connection::get()?.execute("DELETE FROM person", ())?;

    test()
}

#[test]
fn new_record_save_returns_invalid_model_with_errors() -> Result<(), Error> {
    with_validation_schema(|| {
        let draft = Person::builder().name("").build()?;
        let invalid = match draft.save() {
            Err(seekwel::SaveError::Invalid(invalid)) => invalid,
            _ => panic!("expected invalid model"),
        };

        assert_eq!(invalid.name, "");
        assert_eq!(invalid.persisted_id(), None);
        assert!(invalid.is_new_record());
        assert_eq!(
            invalid.errors().on(PersonColumns::Name),
            vec!["can't be blank"]
        );
        assert_eq!(
            invalid.errors().full_messages(),
            vec!["name can't be blank".to_string()]
        );

        Ok(())
    })
}

#[test]
fn persisted_save_returns_invalid_model_with_errors() -> Result<(), Error> {
    with_validation_schema(|| {
        let mut person = Person::builder().name("Pat").create()?;
        person.name.clear();

        let invalid = match person.save() {
            Err(seekwel::SaveError::Invalid(invalid)) => invalid,
            _ => panic!("expected invalid model"),
        };

        assert_eq!(invalid.id, person.id);
        assert_eq!(invalid.persisted_id(), Some(person.id));
        assert!(invalid.is_persisted());
        assert_eq!(
            invalid.errors().on(PersonColumns::Name),
            vec!["can't be blank"]
        );

        Ok(())
    })
}
