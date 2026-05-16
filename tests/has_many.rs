use std::sync::{Mutex, OnceLock};

use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;
use seekwel::{BelongsTo, Comparison, HasMany};

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    #[key = parent_id]
    pets: HasMany<Pet>,
}

#[seekwel::model]
struct Pet {
    id: u64,
    name: String,
    #[key = parent_id]
    owner: BelongsTo<Person>,
}

static HAS_MANY_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn association_save_errors_are_thread_safe_errors() {
    fn assert_thread_safe_error<T: std::error::Error + Send + Sync + 'static>() {}

    assert_thread_safe_error::<
        seekwel::SaveError<Person<seekwel::Invalid<seekwel::NewRecord, PersonColumns>>>,
    >();
    assert_thread_safe_error::<
        seekwel::SaveError<Person<seekwel::Invalid<seekwel::Persisted, PersonColumns>>>,
    >();
}

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

        let from_query = Pet::q(PetColumns::ParentId, Comparison::Eq(owner.id)).all()?;
        assert_eq!(from_query.len(), 2);

        let found_owner = Person::find(owner.id)?;
        assert_eq!(found_owner.pets()?.len(), 2);

        Ok(())
    })
}

#[test]
fn has_many_query_interface_is_scoped_to_the_parent() -> Result<(), Error> {
    with_associations(|| {
        let owner = Person::builder().name("Pat").create()?;
        let other_owner = Person::builder().name("Sam").create()?;

        owner.pets.append(Pet::builder().name("Fido"))?;
        owner.pets.append(Pet::builder().name("Rex"))?;
        other_owner.pets.append(Pet::builder().name("Rex"))?;

        assert_eq!(owner.pets.count()?, 2);
        assert!(owner.pets.exists()?);

        let first_descending = owner.pets.desc(PetColumns::Name).first()?;
        assert_eq!(
            first_descending.map(|pet| pet.name),
            Some("Rex".to_string())
        );

        let scoped_or_names: Vec<_> = owner
            .pets
            .q(PetColumns::Name, Comparison::Eq("Fido"))
            .or(Pet::q(PetColumns::Name, Comparison::Eq("Rex")))
            .order(PetColumns::Name)
            .all()?
            .into_iter()
            .map(|pet| pet.name)
            .collect();
        assert_eq!(scoped_or_names, vec!["Fido".to_string(), "Rex".to_string()]);

        let lazy_names: Result<Vec<_>, _> = owner
            .pets
            .order(PetColumns::Name)
            .lazy()
            .try_iter()?
            .map(|pet| pet.map(|pet| pet.name))
            .collect();
        assert_eq!(lazy_names?, vec!["Fido".to_string(), "Rex".to_string()]);

        let mut chunked_names = Vec::new();
        for pets in owner.pets.order(PetColumns::Name).chunked(1) {
            chunked_names.extend(pets.into_iter().map(|pet| pet.name));
        }
        assert_eq!(chunked_names, vec!["Fido".to_string(), "Rex".to_string()]);

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
            draft.pets.all(),
            Err(Error::InvalidAssociation(_))
        ));
        assert!(matches!(
            draft
                .pets
                .q(PetColumns::Name, Comparison::Eq("Fido"))
                .count(),
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
