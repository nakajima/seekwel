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

#[test]
fn params_create_and_update_only_allowed_columns() -> Result<(), Error> {
    Connection::memory()?;
    Person::create_table()?;
    Pet::create_table()?;
    ManualPerson::create_table()?;

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

    let pet = Pet::create(
        PetParams::new()
            .name("Fido")
            .owner_id(person.id)
            .allow([PetColumns::Name, PetColumns::OwnerId]),
    )?;
    assert_eq!(pet.owner()?.id, person.id);

    let mut manual = ManualPerson::create(
        ManualPersonParams::new()
            .key(7)
            .name("Manual")
            .allow_all(),
    )?;
    assert_eq!(manual.key, 7);
    assert_eq!(manual.name, "Manual");
    assert!(matches!(
        manual.update(ManualPersonParams::new().key(8).allow([ManualPersonColumns::Key])),
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
    let draft = Person::new(params.allow([PersonColumns::Name]))?;

    assert_eq!(draft.name, "Alex");
    assert_eq!(draft.age, None);

    Ok(())
}
