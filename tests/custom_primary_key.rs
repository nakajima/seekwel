use std::sync::{Mutex, OnceLock};

use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;
use seekwel::{BelongsTo, Comparison, HasMany};

#[seekwel::model(table_name = "hyperlink_tombstone", primary_key = "hyperlink_id", auto_increment = false)]
struct HyperlinkTombstone {
    hyperlink_id: i32,
    updated_at: String,
}

#[seekwel::model(table_name = "person_record", primary_key = "person_id")]
struct Person {
    person_id: i32,
    name: String,
    pets: HasMany<Pet, { PetColumns::OWNER_ID }>,
}

#[seekwel::model(table_name = "pet_record", primary_key = "pet_id")]
struct Pet {
    pet_id: i32,
    name: String,
    owner: BelongsTo<Person>,
}

static CUSTOM_PK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_custom_schema(test: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    let _guard = CUSTOM_PK_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(err) => return Err(err),
    }

    HyperlinkTombstone::create_table()?;
    Person::create_table()?;
    Pet::create_table()?;
    Connection::get()?.execute("DELETE FROM hyperlink_tombstone", ())?;
    Connection::get()?.execute("DELETE FROM pet_record", ())?;
    Connection::get()?.execute("DELETE FROM person_record", ())?;

    test()
}

#[test]
fn manual_primary_keys_and_custom_table_names_round_trip() -> Result<(), Error> {
    with_custom_schema(|| {
        let tombstone = HyperlinkTombstone::builder()
            .hyperlink_id(42)
            .updated_at("2026-01-01T00:00:00".to_string())
            .create()?;

        assert_eq!(tombstone.hyperlink_id, 42);

        let found = HyperlinkTombstone::find(42_i32)?;
        assert_eq!(found.hyperlink_id, 42);
        assert_eq!(found.updated_at, "2026-01-01T00:00:00");

        let schema = seekwel::schema::SchemaBuilder::new()
            .model::<HyperlinkTombstone>()
            .build()?;
        assert_eq!(schema.tables[0].name, "hyperlink_tombstone");
        assert_eq!(schema.tables[0].primary_key.name, "hyperlink_id");

        Ok(())
    })
}

#[test]
fn auto_generated_i32_primary_keys_work_with_find_and_save() -> Result<(), Error> {
    with_custom_schema(|| {
        let mut person = Person::builder().name("Pat").create()?;

        assert_eq!(person.person_id, 1);
        let found = Person::find(person.person_id)?;
        assert_eq!(found.person_id, person.person_id);
        assert_eq!(found.name, "Pat");

        person.name = "Patricia".to_string();
        person.save()?;
        let refreshed = Person::find(1_i32)?;
        assert_eq!(refreshed.name, "Patricia");

        Ok(())
    })
}

#[test]
fn associations_still_work_with_custom_i32_primary_keys() -> Result<(), Error> {
    with_custom_schema(|| {
        let owner = Person::builder().name("Pat").create()?;
        let pet = owner.pets.append(Pet::builder().name("Fido"))?;

        assert_eq!(pet.pet_id, 1);
        assert_eq!(pet.owner.id(), owner.person_id as u64);
        assert_eq!(pet.owner()?.person_id, owner.person_id);

        let pets = Pet::q(PetColumns::OwnerId, Comparison::Eq(owner.person_id)).all()?;
        assert_eq!(pets.len(), 1);
        assert_eq!(pets[0].pet_id, pet.pet_id);

        let found_owner = Person::find(owner.person_id)?;
        assert_eq!(found_owner.pets()?.len(), 1);

        Ok(())
    })
}
