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

#[test]
fn find_and_query_records() -> Result<(), Error> {
    Connection::memory()?;
    Person::create_table()?;

    let pat = Person::builder().name("Pat").age(Some(20)).create()?;
    let sam = Person::builder().name("Sam").age(Some(30)).create()?;
    let alex = Person::builder().name("Alex").create()?;

    let found = Person::find(pat.id)?;
    assert_eq!(found.name, "Pat");
    assert_eq!(found.age, Some(20));

    let everyone = Person::all()?;
    assert_eq!(everyone.len(), 3);

    let first_person = Person::first()?;
    assert!(first_person.is_some());

    let model_and = Person::and(Person::q(PersonColumns::Name, Comparison::Eq("Pat"))).all()?;
    assert_eq!(model_and.len(), 1);
    assert_eq!(model_and[0].id, pat.id);

    let model_or = Person::or(Person::q(PersonColumns::Name, Comparison::Eq("Pat"))).all()?;
    assert_eq!(model_or.len(), 1);
    assert_eq!(model_or[0].id, pat.id);

    let mut model_iter_ids: Vec<_> = Person::iter()?.map(|person| person.id).collect();
    model_iter_ids.sort_unstable();
    assert_eq!(model_iter_ids, vec![pat.id, sam.id, alex.id]);

    let model_lazy_try_ids: Result<Vec<_>, _> = Person::lazy()
        .try_iter()?
        .map(|person| person.map(|person| person.id))
        .collect();
    let mut model_lazy_try_ids = model_lazy_try_ids?;
    model_lazy_try_ids.sort_unstable();
    assert_eq!(model_lazy_try_ids, vec![pat.id, sam.id, alex.id]);

    let mut model_chunked_ids = Vec::new();
    for people in Person::chunked(2) {
        model_chunked_ids.extend(people.into_iter().map(|person| person.id));
    }
    model_chunked_ids.sort_unstable();
    assert_eq!(model_chunked_ids, vec![pat.id, sam.id, alex.id]);

    let by_name = Person::q(PersonColumns::Name, Comparison::Eq("Sam")).first()?;
    assert_eq!(by_name.map(|person| person.id), Some(sam.id));

    let by_id = Person::q(PersonColumns::Id, Comparison::Eq(pat.id)).first()?;
    assert_eq!(by_id.map(|person| person.name), Some("Pat".to_string()));

    let not_pat = Person::q(PersonColumns::Name, Comparison::Ne("Pat")).all()?;
    assert_eq!(not_pat.len(), 2);
    assert!(not_pat.iter().any(|person| person.id == sam.id));
    assert!(not_pat.iter().any(|person| person.id == alex.id));

    let older = Person::q(PersonColumns::Age, Comparison::Gt(20)).all()?;
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].name, "Sam");

    let adults = Person::q(PersonColumns::Age, Comparison::Gte(20)).all()?;
    assert_eq!(adults.len(), 2);
    assert!(adults.iter().any(|person| person.id == pat.id));
    assert!(adults.iter().any(|person| person.id == sam.id));

    let younger_than_thirty = Person::q(PersonColumns::Age, Comparison::Lt(30)).all()?;
    assert_eq!(younger_than_thirty.len(), 1);
    assert_eq!(younger_than_thirty[0].id, pat.id);

    let twenty_or_younger = Person::q(PersonColumns::Age, Comparison::Lte(20)).all()?;
    assert_eq!(twenty_or_younger.len(), 1);
    assert_eq!(twenty_or_younger[0].id, pat.id);

    let age_and_name = Person::q(PersonColumns::Age, Comparison::Gte(20))
        .and(Person::q(PersonColumns::Name, Comparison::Eq("Pat")))
        .all()?;
    assert_eq!(age_and_name.len(), 1);
    assert_eq!(age_and_name[0].id, pat.id);

    let chained_q = Person::q(PersonColumns::Age, Comparison::Gte(20))
        .q(PersonColumns::Name, Comparison::Eq("Pat"))
        .all()?;
    assert_eq!(chained_q.len(), 1);
    assert_eq!(chained_q[0].id, pat.id);

    let pat_or_sam = Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .all()?;
    assert_eq!(pat_or_sam.len(), 2);
    assert!(pat_or_sam.iter().any(|person| person.id == pat.id));
    assert!(pat_or_sam.iter().any(|person| person.id == sam.id));

    let grouped = Person::q(PersonColumns::Age, Comparison::Gte(20))
        .and(
            Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
                .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam"))),
        )
        .all()?;
    assert_eq!(grouped.len(), 2);
    assert!(grouped.iter().any(|person| person.id == pat.id));
    assert!(grouped.iter().any(|person| person.id == sam.id));

    let chained_after_or = Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .q(PersonColumns::Age, Comparison::Gt(20))
        .all()?;
    assert_eq!(chained_after_or.len(), 1);
    assert_eq!(chained_after_or[0].id, sam.id);

    let mut iterated_ids = Vec::new();
    for person in Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .iter()?
    {
        iterated_ids.push(person.id);
    }
    iterated_ids.sort_unstable();
    assert_eq!(iterated_ids, vec![pat.id, sam.id]);

    let mut eager_try_ids: Vec<_> = Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .try_iter()?
        .map(|person| person.id)
        .collect();
    eager_try_ids.sort_unstable();
    assert_eq!(eager_try_ids, vec![pat.id, sam.id]);

    let mut direct_ids = Vec::new();
    for person in Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
    {
        direct_ids.push(person.id);
    }
    direct_ids.sort_unstable();
    assert_eq!(direct_ids, vec![pat.id, sam.id]);

    let mut lazy_ids = Vec::new();
    for person in Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .lazy()
        .iter()?
    {
        lazy_ids.push(person.id);
    }
    lazy_ids.sort_unstable();
    assert_eq!(lazy_ids, vec![pat.id, sam.id]);

    let lazy_try_ids: Result<Vec<_>, _> = Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .lazy()
        .try_iter()?
        .map(|person| person.map(|person| person.id))
        .collect();
    let mut lazy_try_ids = lazy_try_ids?;
    lazy_try_ids.sort_unstable();
    assert_eq!(lazy_try_ids, vec![pat.id, sam.id]);

    let mut direct_lazy_ids = Vec::new();
    for person in Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .lazy()
    {
        direct_lazy_ids.push(person.id);
    }
    direct_lazy_ids.sort_unstable();
    assert_eq!(direct_lazy_ids, vec![pat.id, sam.id]);

    let mut chunked_iter_ids = Vec::new();
    for people in Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .chunked(2)
        .iter()?
    {
        chunked_iter_ids.extend(people.into_iter().map(|person| person.id));
    }
    chunked_iter_ids.sort_unstable();
    assert_eq!(chunked_iter_ids, vec![pat.id, sam.id]);

    let chunked_ids: Result<Vec<_>, _> = Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .chunked(2)
        .try_iter()?
        .map(|people| {
            people.map(|people| {
                people
                    .into_iter()
                    .map(|person| person.id)
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut chunked_ids = chunked_ids?.into_iter().flatten().collect::<Vec<_>>();
    chunked_ids.sort_unstable();
    assert_eq!(chunked_ids, vec![pat.id, sam.id]);

    let mut direct_chunked_ids = Vec::new();
    for people in Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
        .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam")))
        .chunked(2)
    {
        direct_chunked_ids.extend(people.into_iter().map(|person| person.id));
    }
    direct_chunked_ids.sort_unstable();
    assert_eq!(direct_chunked_ids, vec![pat.id, sam.id]);

    let missing = Person::q(PersonColumns::Name, Comparison::Eq("Taylor")).first()?;
    assert!(missing.is_none());

    let null_age = Person::q(PersonColumns::Age, Comparison::Eq(None::<u8>)).first()?;
    assert_eq!(null_age.map(|person| person.id), Some(alex.id));

    let not_null_age = Person::q(PersonColumns::Age, Comparison::Ne(None::<u8>)).all()?;
    assert_eq!(not_null_age.len(), 2);
    assert!(not_null_age.iter().any(|person| person.id == pat.id));
    assert!(not_null_age.iter().any(|person| person.id == sam.id));

    let invalid_null_comparison = Person::q(PersonColumns::Age, Comparison::Gt(None::<u8>)).first();
    assert!(matches!(
        invalid_null_comparison,
        Err(Error::InvalidQuery(message)) if message.contains("Gt comparisons do not support NULL")
    ));

    Ok(())
}
