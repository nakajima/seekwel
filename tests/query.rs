use seekwel::Comparison;
use seekwel::connection::Connection;
use seekwel::error::Error;

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

    let by_name = Person::q("name", Comparison::Eq("Sam")).first()?;
    assert_eq!(by_name.map(|person| person.id), Some(sam.id));

    let by_id = Person::q("id", Comparison::Eq(pat.id)).first()?;
    assert_eq!(by_id.map(|person| person.name), Some("Pat".to_string()));

    let not_pat = Person::q("name", Comparison::Ne("Pat")).all()?;
    assert_eq!(not_pat.len(), 2);
    assert!(not_pat.iter().any(|person| person.id == sam.id));
    assert!(not_pat.iter().any(|person| person.id == alex.id));

    let older = Person::q("age", Comparison::Gt(20)).all()?;
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].name, "Sam");

    let adults = Person::q("age", Comparison::Gte(20)).all()?;
    assert_eq!(adults.len(), 2);
    assert!(adults.iter().any(|person| person.id == pat.id));
    assert!(adults.iter().any(|person| person.id == sam.id));

    let younger_than_thirty = Person::q("age", Comparison::Lt(30)).all()?;
    assert_eq!(younger_than_thirty.len(), 1);
    assert_eq!(younger_than_thirty[0].id, pat.id);

    let twenty_or_younger = Person::q("age", Comparison::Lte(20)).all()?;
    assert_eq!(twenty_or_younger.len(), 1);
    assert_eq!(twenty_or_younger[0].id, pat.id);

    let age_and_name = Person::q("age", Comparison::Gte(20))
        .and(Person::q("name", Comparison::Eq("Pat")))
        .all()?;
    assert_eq!(age_and_name.len(), 1);
    assert_eq!(age_and_name[0].id, pat.id);

    let chained_q = Person::q("age", Comparison::Gte(20))
        .q("name", Comparison::Eq("Pat"))
        .all()?;
    assert_eq!(chained_q.len(), 1);
    assert_eq!(chained_q[0].id, pat.id);

    let pat_or_sam = Person::q("name", Comparison::Eq("Pat"))
        .or(Person::q("name", Comparison::Eq("Sam")))
        .all()?;
    assert_eq!(pat_or_sam.len(), 2);
    assert!(pat_or_sam.iter().any(|person| person.id == pat.id));
    assert!(pat_or_sam.iter().any(|person| person.id == sam.id));

    let grouped = Person::q("age", Comparison::Gte(20))
        .and(Person::q("name", Comparison::Eq("Pat")).or(Person::q("name", Comparison::Eq("Sam"))))
        .all()?;
    assert_eq!(grouped.len(), 2);
    assert!(grouped.iter().any(|person| person.id == pat.id));
    assert!(grouped.iter().any(|person| person.id == sam.id));

    let chained_after_or = Person::q("name", Comparison::Eq("Pat"))
        .or(Person::q("name", Comparison::Eq("Sam")))
        .q("age", Comparison::Gt(20))
        .all()?;
    assert_eq!(chained_after_or.len(), 1);
    assert_eq!(chained_after_or[0].id, sam.id);

    let mut iterated_ids = Vec::new();
    for person in Person::q("name", Comparison::Eq("Pat"))
        .or(Person::q("name", Comparison::Eq("Sam")))
        .iter()?
    {
        iterated_ids.push(person.id);
    }
    iterated_ids.sort_unstable();
    assert_eq!(iterated_ids, vec![pat.id, sam.id]);

    let missing = Person::q("name", Comparison::Eq("Taylor")).first()?;
    assert!(missing.is_none());

    let null_age = Person::q("age", Comparison::Eq(None::<u8>)).first()?;
    assert_eq!(null_age.map(|person| person.id), Some(alex.id));

    let not_null_age = Person::q("age", Comparison::Ne(None::<u8>)).all()?;
    assert_eq!(not_null_age.len(), 2);
    assert!(not_null_age.iter().any(|person| person.id == pat.id));
    assert!(not_null_age.iter().any(|person| person.id == sam.id));

    let invalid_column = Person::q("not_a_column", Comparison::Eq(1)).first();
    assert!(matches!(
        invalid_column,
        Err(Error::InvalidQuery(message)) if message.contains("unknown column `not_a_column`")
    ));

    let invalid_null_comparison = Person::q("age", Comparison::Gt(None::<u8>)).first();
    assert!(matches!(
        invalid_null_comparison,
        Err(Error::InvalidQuery(message)) if message.contains("Gt comparisons do not support NULL")
    ));

    let invalid_iter = Person::q("not_a_column", Comparison::Eq(1)).iter();
    assert!(matches!(
        invalid_iter,
        Err(Error::InvalidQuery(message)) if message.contains("unknown column `not_a_column`")
    ));

    Ok(())
}
