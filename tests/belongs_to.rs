use std::sync::{Mutex, OnceLock};

use seekwel::BelongsTo;
use seekwel::Comparison;
use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Pet {
    id: u64,
    name: String,
    owner: BelongsTo<Person>,
    sitter: Option<BelongsTo<Person>>,
}

static ASSOCIATION_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_associations(test: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    let _guard = ASSOCIATION_TEST_LOCK
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
fn belongs_to_round_trips_loads_and_queries_by_raw_fk() -> Result<(), Error> {
    with_associations(|| {
        let pat = Person::builder().name("Pat").create()?;
        let sam = Person::builder().name("Sam").create()?;

        let pet = Pet::builder()
            .name("Fido")
            .owner(pat.clone())
            .sitter(Some(&sam))
            .create()?;

        assert_eq!(pet.owner.id(), pat.id);
        assert_eq!(pet.sitter.as_ref().map(|sitter| sitter.id()), Some(sam.id));

        let owner = pet.owner()?;
        assert_eq!(owner.id, pat.id);
        assert_eq!(owner.name, "Pat");

        let sitter = pet.sitter()?;
        assert_eq!(sitter.map(|person| person.id), Some(sam.id));

        let found = Pet::find(pet.id)?;
        assert_eq!(found.owner.id(), pat.id);
        assert_eq!(
            found.sitter.as_ref().map(|sitter| sitter.id()),
            Some(sam.id)
        );
        assert_eq!(found.owner()?.id, pat.id);

        let owned_by_pat = Pet::q(PetColumns::OwnerId, Comparison::Eq(pat.id)).all()?;
        assert_eq!(owned_by_pat.len(), 1);
        assert_eq!(owned_by_pat[0].id, pet.id);

        let with_sam_as_sitter = Pet::q(PetColumns::SitterId, Comparison::Eq(sam.id)).first()?;
        assert_eq!(with_sam_as_sitter.map(|pet| pet.id), Some(pet.id));

        let no_sitter = Pet::builder().name("Solo").owner(&pat).create()?;
        let null_sitter = Pet::q(PetColumns::SitterId, Comparison::IsNull).all()?;
        assert_eq!(null_sitter.len(), 1);
        assert_eq!(null_sitter[0].id, no_sitter.id);
        assert!(no_sitter.sitter()?.is_none());

        Ok(())
    })
}

#[test]
fn belongs_to_caches_loaded_parent_until_reload() -> Result<(), Error> {
    with_associations(|| {
        let pat = Person::builder().name("Pat").create()?;
        let mut pet = Pet::builder().name("Cached").owner(pat.id).create()?;

        let owner = pet.owner()?;
        assert_eq!(owner.id, pat.id);

        pat.delete()?;

        let cached_owner = pet.owner()?;
        assert_eq!(cached_owner.id, owner.id);
        assert_eq!(cached_owner.name, owner.name);

        pet.reload()?;
        assert!(matches!(pet.owner(), Err(Error::Sqlite(_))));

        Ok(())
    })
}

#[test]
fn reassigning_belongs_to_seeds_or_clears_the_cache() -> Result<(), Error> {
    with_associations(|| {
        let pat = Person::builder().name("Pat").create()?;
        let sam = Person::builder().name("Sam").create()?;
        let mut pet = Pet::builder().name("Switcher").owner(pat.id).create()?;

        pet.owner = sam.clone().into();
        sam.delete()?;
        assert_eq!(pet.owner()?.name, "Sam");

        pet.owner = pat.id.into();
        pat.delete()?;
        assert!(matches!(pet.owner(), Err(Error::Sqlite(_))));

        Ok(())
    })
}
