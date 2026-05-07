use std::sync::{Mutex, OnceLock};

use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;
use seekwel::{BelongsTo, Comparison, HasMany};

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    pets: HasMany<Pet, { PetColumns::OWNER_ID }>,
}

#[seekwel::model]
struct Pet {
    id: u64,
    name: String,
    owner: BelongsTo<Person>,
}

static HAS_MANY_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_associations(test: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    let _guard = HAS_MANY_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(err) => return Err(err),
    }

    Person::create_table()?;
    Pet::create_table()?;
    Connection::get()?.execute("DELETE FROM pet", ())?;
    Connection::get()?.execute("DELETE FROM person", ())?;

    test()
}

#[test]
fn has_many_loads_appends_and_queries_children() -> Result<(), Error> {
    with_associations(|| {
        let owner = Person::builder().name("Pat").create()?;

        assert_eq!(owner.pets()?.len(), 0);

        let fido = owner.pets.append(Pet::builder().name("Fido"))?;
        let rex = owner.pets.append(Pet::builder().name("Rex"))?;

        assert_eq!(fido.owner.id(), owner.id);
        assert_eq!(rex.owner.id(), owner.id);

        let pets = owner.pets()?;
        assert_eq!(pets.len(), 2);
        assert_eq!(pets[0].owner()?.id, owner.id);
        assert_eq!(pets[1].owner()?.id, owner.id);

        let from_query = Pet::q(PetColumns::OwnerId, Comparison::Eq(owner.id)).all()?;
        assert_eq!(from_query.len(), 2);

        let found_owner = Person::find(owner.id)?;
        assert_eq!(found_owner.pets()?.len(), 2);

        Ok(())
    })
}

#[test]
fn has_many_requires_a_persisted_parent() -> Result<(), Error> {
    with_associations(|| {
        let draft = Person::builder().name("Draft").build()?;

        assert!(matches!(
            draft.pets.load(),
            Err(Error::InvalidAssociation(_))
        ));
        assert!(matches!(
            draft.pets.append(Pet::builder().name("Fido")),
            Err(Error::InvalidAssociation(_))
        ));

        Ok(())
    })
}

#[test]
fn has_many_cache_is_reset_when_parent_is_reloaded() -> Result<(), Error> {
    with_associations(|| {
        let mut owner = Person::builder().name("Pat").create()?;
        owner.pets.append(Pet::builder().name("Fido"))?;

        assert_eq!(owner.pets()?.len(), 1);

        Connection::get()?.execute("DELETE FROM pet", ())?;

        assert_eq!(
            owner.pets()?.len(),
            1,
            "cache should still hold the loaded children before reload",
        );

        owner.reload()?;
        assert_eq!(
            owner.pets()?.len(),
            0,
            "reload should replace the parent and reset its HasMany cache",
        );

        Ok(())
    })
}

#[test]
fn has_many_keeps_a_loaded_cache_coherent_after_append() -> Result<(), Error> {
    with_associations(|| {
        let owner = Person::builder().name("Pat").create()?;

        assert_eq!(owner.pets()?.len(), 0);

        let pet = owner.pets.append(Pet::builder().name("Cached"))?;
        pet.delete()?;

        let cached = owner.pets()?;
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name, "Cached");

        owner.pets.clear_cache();
        assert_eq!(owner.pets()?.len(), 0);

        Ok(())
    })
}
