use seekwel::BelongsTo;
use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    age: Option<u8>,
}

#[seekwel::model]
struct Pet {
    id: u64,
    name: String,
    owner: BelongsTo<Person>,
}

#[seekwel::model(primary_key = "key", auto_increment = false)]
struct ManualPerson {
    key: i32,
    name: String,
}

#[seekwel::model(table_name = "apps")]
struct App {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Todo {
    id: u64,
    title: String,
    done: bool,
}

#[test]
fn params_create_and_update_only_allowed_columns() -> Result<(), Error> {
    Connection::memory()?;
    Person::create_table()?;
    Pet::create_table()?;
    ManualPerson::create_table()?;
    Todo::create_table()?;

    let draft = Person::new(
        PersonParams::new()
            .name("Pat")
            .age(Some(20))
            .allow([PersonColumns::Name]),
    )?;
    assert_eq!(draft.name, "Pat");
    assert_eq!(draft.age, None);

    let mut person = Person::create(
        PersonParams::new()
            .name("Pat")
            .age(Some(20))
            .allow([PersonColumns::Name, PersonColumns::Age]),
    )?;
    assert_eq!(person.name, "Pat");
    assert_eq!(person.age, Some(20));

    person.update(
        PersonParams::new()
            .name("Ignored")
            .age(Some(21))
            .allow([PersonColumns::Age]),
    )?;
    assert_eq!(person.name, "Pat");
    assert_eq!(person.age, Some(21));

    let refreshed = Person::find(person.id)?;
    assert_eq!(refreshed.name, "Pat");
    assert_eq!(refreshed.age, Some(21));

    let mut todo = Todo::create(
        TodoParams::new()
            .title("Ship it")
            .allow([TodoColumns::Title, TodoColumns::Done]),
    )?;
    assert!(!todo.done);

    todo.update(TodoParams::new().done(true).allow([TodoColumns::Done]))?;
    assert!(todo.done);

    todo.update(TodoParams::new().allow([TodoColumns::Done]))?;
    assert!(!todo.done);
    assert!(!Todo::find(todo.id)?.done);

    assert!(matches!(
        Todo::new(TodoParams::new().title("No done").allow([TodoColumns::Title])),
        Err(Error::MissingField(field)) if field == "done"
    ));

    let pet = Pet::create(
        PetParams::new()
            .name("Fido")
            .owner_id(person.id)
            .allow([PetColumns::Name, PetColumns::OwnerId]),
    )?;
    assert_eq!(pet.owner()?.id, person.id);

    let mut manual =
        ManualPerson::create(ManualPersonParams::new().key(7).name("Manual").allow_all())?;
    assert_eq!(manual.key, 7);
    assert_eq!(manual.name, "Manual");
    assert!(matches!(
        manual.update(
            ManualPersonParams::new()
                .key(8)
                .allow([ManualPersonColumns::Key])
        ),
        Err(seekwel::SaveError::Error(Error::InvalidParams(_)))
    ));

    assert!(matches!(
        Person::new(
            PersonParams::new()
                .name("Nope")
                .allow([PersonColumns::Id, PersonColumns::Name])
        ),
        Err(Error::InvalidParams(_))
    ));
    assert!(matches!(
        person.update(PersonParams::new().name("Nope").allow([PersonColumns::Id])),
        Err(seekwel::SaveError::Error(Error::InvalidParams(_)))
    ));
    assert!(matches!(
        Person::new(PersonParams::new().age(Some(1)).allow([PersonColumns::Age])),
        Err(Error::MissingField(field)) if field == "name"
    ));

    let allowed = PersonParams::new().name("Sam").allow_all();
    assert!(allowed.allows(PersonColumns::Name));
    assert!(!allowed.allows(PersonColumns::Id));

    Ok(())
}

#[cfg(feature = "serde")]
#[test]
fn params_deserialize_with_serde() -> Result<(), Error> {
    use seekwel::__private::serde::Deserialize;
    use seekwel::__private::serde::de::value::{Error as DeError, MapDeserializer};

    let deserializer = MapDeserializer::<_, DeError>::new([("name", "Alex")].into_iter());
    let params = PersonParams::deserialize(deserializer).unwrap();
    assert!(matches!(
        Person::new(params.allow([PersonColumns::Name])),
        Err(Error::MissingField(field)) if field == "name"
    ));

    let deserializer = MapDeserializer::<_, DeError>::new(
        [("person[name]", "Sam"), ("person[age]", "42")].into_iter(),
    );
    let params = PersonParams::deserialize(deserializer).unwrap();
    let draft = Person::new(params.allow([PersonColumns::Name, PersonColumns::Age]))?;

    assert_eq!(draft.name, "Sam");
    assert_eq!(draft.age, Some(42));

    let deserializer = MapDeserializer::<_, DeError>::new([("app[name]", "Calendar")].into_iter());
    let params = AppParams::deserialize(deserializer).unwrap();
    let draft = App::new(params.allow([AppColumns::Name]))?;

    assert_eq!(draft.name, "Calendar");

    for value in ["1", "true", "on", "yes", "TRUE", "YES"] {
        let deserializer = MapDeserializer::<_, DeError>::new(
            [("todo[title]", "Truthy"), ("todo[done]", value)].into_iter(),
        );
        let params = TodoParams::deserialize(deserializer).unwrap();
        let draft = Todo::new(params.allow([TodoColumns::Title, TodoColumns::Done]))?;
        assert!(draft.done, "expected {value:?} to deserialize as true");
    }

    for value in ["0", "false", "off", "no", "FALSE", "NO"] {
        let deserializer = MapDeserializer::<_, DeError>::new(
            [("todo[title]", "Falsy"), ("todo[done]", value)].into_iter(),
        );
        let params = TodoParams::deserialize(deserializer).unwrap();
        let draft = Todo::new(params.allow([TodoColumns::Title, TodoColumns::Done]))?;
        assert!(!draft.done, "expected {value:?} to deserialize as false");
    }

    let deserializer =
        MapDeserializer::<_, DeError>::new([("todo[title]", "Unchecked")].into_iter());
    let params = TodoParams::deserialize(deserializer).unwrap();
    let draft = Todo::new(params.allow([TodoColumns::Title, TodoColumns::Done]))?;
    assert!(!draft.done);

    let deserializer = MapDeserializer::<_, DeError>::new([("todo[done]", "maybe")].into_iter());
    assert!(TodoParams::deserialize(deserializer).is_err());

    Ok(())
}
