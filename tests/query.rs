use std::sync::{Mutex, OnceLock};

use seekwel::Comparison;
use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    age: Option<u8>,
}

struct PeopleFixture {
    pat: Person,
    sam: Person,
    alex: Person,
}

static QUERY_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_people(test: impl FnOnce(&PeopleFixture) -> Result<(), Error>) -> Result<(), Error> {
    let _guard = QUERY_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(err) => return Err(err),
    }

    Person::create_table()?;
    Connection::get()?.execute("DELETE FROM person", ())?;

    let fixture = PeopleFixture {
        pat: Person::builder().name("Pat").age(Some(20)).create()?,
        sam: Person::builder().name("Sam").age(Some(30)).create()?,
        alex: Person::builder().name("Alex").create()?,
    };

    test(&fixture)
}

fn pat_or_sam() -> seekwel::Query<Person> {
    Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
}

fn sorted_ids(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids
}

#[test]
fn model_level_queries_and_lookup_work() -> Result<(), Error> {
    with_people(|people| {
        let found = Person::find(people.pat.id)?;
        assert_eq!(found.name, "Pat");
        assert_eq!(found.age, Some(20));

        let everyone = Person::all()?;
        assert_eq!(
            sorted_ids(everyone.into_iter().map(|person| person.id).collect()),
            vec![people.pat.id, people.sam.id, people.alex.id,]
        );

        assert!(Person::first()?.is_some());

        let model_and = Person::and(Person::q(PersonColumns::Name, Comparison::Eq("Pat"))).all()?;
        assert_eq!(model_and.len(), 1);
        assert_eq!(model_and[0].id, people.pat.id);

        let model_or = Person::or(Person::q(PersonColumns::Name, Comparison::Eq("Pat"))).all()?;
        assert_eq!(model_or.len(), 1);
        assert_eq!(model_or[0].id, people.pat.id);

        Ok(())
    })
}

#[test]
fn comparison_operators_filter_records() -> Result<(), Error> {
    with_people(|people| {
        let by_name = Person::q(PersonColumns::Name, Comparison::Eq("Sam")).first()?;
        assert_eq!(by_name.map(|person| person.id), Some(people.sam.id));

        let by_id = Person::q(PersonColumns::Id, Comparison::Eq(people.pat.id)).first()?;
        assert_eq!(by_id.map(|person| person.name), Some("Pat".to_string()));

        let not_pat = Person::q(PersonColumns::Name, Comparison::Ne("Pat")).all()?;
        assert_eq!(
            sorted_ids(not_pat.into_iter().map(|person| person.id).collect()),
            vec![people.sam.id, people.alex.id,]
        );

        let older = Person::q(PersonColumns::Age, Comparison::Gt(20)).all()?;
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].id, people.sam.id);

        let adults = Person::q(PersonColumns::Age, Comparison::Gte(20)).all()?;
        assert_eq!(
            sorted_ids(adults.into_iter().map(|person| person.id).collect()),
            vec![people.pat.id, people.sam.id,]
        );

        let younger_than_thirty = Person::q(PersonColumns::Age, Comparison::Lt(30)).all()?;
        assert_eq!(younger_than_thirty.len(), 1);
        assert_eq!(younger_than_thirty[0].id, people.pat.id);

        let twenty_or_younger = Person::q(PersonColumns::Age, Comparison::Lte(20)).all()?;
        assert_eq!(twenty_or_younger.len(), 1);
        assert_eq!(twenty_or_younger[0].id, people.pat.id);

        let missing = Person::q(PersonColumns::Name, Comparison::Eq("Taylor")).first()?;
        assert!(missing.is_none());

        Ok(())
    })
}

#[test]
fn boolean_query_composition_respects_grouping() -> Result<(), Error> {
    with_people(|people| {
        let age_and_name = Person::q(PersonColumns::Age, Comparison::Gte(20))
            .and(Person::q(PersonColumns::Name, Comparison::Eq("Pat")))
            .all()?;
        assert_eq!(age_and_name.len(), 1);
        assert_eq!(age_and_name[0].id, people.pat.id);

        let chained_q = Person::q(PersonColumns::Age, Comparison::Gte(20))
            .q(PersonColumns::Name, Comparison::Eq("Pat"))
            .all()?;
        assert_eq!(chained_q.len(), 1);
        assert_eq!(chained_q[0].id, people.pat.id);

        let pat_or_sam_ids = pat_or_sam().all()?;
        assert_eq!(
            sorted_ids(pat_or_sam_ids.into_iter().map(|person| person.id).collect()),
            vec![people.pat.id, people.sam.id,]
        );

        let grouped = Person::q(PersonColumns::Age, Comparison::Gte(20))
            .and(
                Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
                    .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam"))),
            )
            .all()?;
        assert_eq!(
            sorted_ids(grouped.into_iter().map(|person| person.id).collect()),
            vec![people.pat.id, people.sam.id,]
        );

        let chained_after_or = pat_or_sam()
            .q(PersonColumns::Age, Comparison::Gt(20))
            .all()?;
        assert_eq!(chained_after_or.len(), 1);
        assert_eq!(chained_after_or[0].id, people.sam.id);

        Ok(())
    })
}

#[test]
fn unfiltered_iteration_modes_return_all_records() -> Result<(), Error> {
    with_people(|people| {
        let model_iter_ids: Vec<_> = Person::iter()?.map(|person| person.id).collect();
        assert_eq!(
            sorted_ids(model_iter_ids),
            vec![people.pat.id, people.sam.id, people.alex.id]
        );

        let model_lazy_try_ids: Result<Vec<_>, _> = Person::lazy()
            .try_iter()?
            .map(|person| person.map(|person| person.id))
            .collect();
        assert_eq!(
            sorted_ids(model_lazy_try_ids?),
            vec![people.pat.id, people.sam.id, people.alex.id]
        );

        let mut model_chunked_ids = Vec::new();
        for people_chunk in Person::chunked(2) {
            model_chunked_ids.extend(people_chunk.into_iter().map(|person| person.id));
        }
        assert_eq!(
            sorted_ids(model_chunked_ids),
            vec![people.pat.id, people.sam.id, people.alex.id]
        );

        Ok(())
    })
}

#[test]
fn filtered_iteration_modes_return_matching_records() -> Result<(), Error> {
    with_people(|people| {
        let expected = vec![people.pat.id, people.sam.id];

        let mut iterated_ids = Vec::new();
        for person in pat_or_sam().iter()? {
            iterated_ids.push(person.id);
        }
        assert_eq!(sorted_ids(iterated_ids), expected);

        let eager_try_ids: Vec<_> = pat_or_sam().try_iter()?.map(|person| person.id).collect();
        assert_eq!(sorted_ids(eager_try_ids), expected);

        let mut direct_ids = Vec::new();
        for person in pat_or_sam() {
            direct_ids.push(person.id);
        }
        assert_eq!(sorted_ids(direct_ids), expected);

        let mut lazy_ids = Vec::new();
        for person in pat_or_sam().lazy().iter()? {
            lazy_ids.push(person.id);
        }
        assert_eq!(sorted_ids(lazy_ids), expected);

        let lazy_try_ids: Result<Vec<_>, _> = pat_or_sam()
            .lazy()
            .try_iter()?
            .map(|person| person.map(|person| person.id))
            .collect();
        assert_eq!(sorted_ids(lazy_try_ids?), expected);

        let mut direct_lazy_ids = Vec::new();
        for person in pat_or_sam().lazy() {
            direct_lazy_ids.push(person.id);
        }
        assert_eq!(sorted_ids(direct_lazy_ids), expected);

        let mut chunked_iter_ids = Vec::new();
        for people_chunk in pat_or_sam().chunked(2).iter()? {
            chunked_iter_ids.extend(people_chunk.into_iter().map(|person| person.id));
        }
        assert_eq!(sorted_ids(chunked_iter_ids), expected);

        let chunked_ids: Result<Vec<_>, _> = pat_or_sam()
            .chunked(2)
            .try_iter()?
            .map(|people_chunk| {
                people_chunk.map(|people_chunk| {
                    people_chunk
                        .into_iter()
                        .map(|person| person.id)
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        assert_eq!(
            sorted_ids(chunked_ids?.into_iter().flatten().collect()),
            expected
        );

        let mut direct_chunked_ids = Vec::new();
        for people_chunk in pat_or_sam().chunked(2) {
            direct_chunked_ids.extend(people_chunk.into_iter().map(|person| person.id));
        }
        assert_eq!(sorted_ids(direct_chunked_ids), expected);

        Ok(())
    })
}

#[test]
fn null_comparisons_are_supported_for_optional_columns() -> Result<(), Error> {
    with_people(|people| {
        let null_age = Person::q(PersonColumns::Age, Comparison::Eq(None::<u8>)).first()?;
        assert_eq!(null_age.map(|person| person.id), Some(people.alex.id));

        let not_null_age = Person::q(PersonColumns::Age, Comparison::Ne(None::<u8>)).all()?;
        assert_eq!(
            sorted_ids(not_null_age.into_iter().map(|person| person.id).collect()),
            vec![people.pat.id, people.sam.id]
        );

        let invalid_null_comparison =
            Person::q(PersonColumns::Age, Comparison::Gt(None::<u8>)).first();
        assert!(matches!(
            invalid_null_comparison,
            Err(Error::InvalidQuery(message)) if message.contains("Gt comparisons do not support NULL")
        ));

        Ok(())
    })
}
