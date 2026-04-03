use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    age: Option<u8>,
}

fn expect_new_record(_: &Person<seekwel::NewRecord>) {}
fn expect_persisted(_: &Person) {}

#[test]
fn builder_save_reload_and_delete() -> Result<(), Error> {
    Connection::memory()?;
    Person::create_table()?;

    let draft = Person::builder().name("Pat").age(Some(100)).build()?;
    expect_new_record(&draft);
    assert_eq!(draft.name, "Pat");
    assert_eq!(draft.age, Some(100));
    assert_eq!(draft.id, 0);

    let mut person = draft.save()?;
    expect_persisted(&person);
    assert_eq!(person.id, 1);

    let id = person.id;
    person.reload()?;
    assert_eq!(person.id, id);
    assert_eq!(person.name, "Pat");
    assert_eq!(person.age, Some(100));

    person.name = "Patricia".to_string();
    person.age = None;
    person.save()?;

    let refreshed = Person::find(person.id)?;
    assert_eq!(refreshed.name, "Patricia");
    assert_eq!(refreshed.age, None);

    person.reload()?;
    assert_eq!(person.name, "Patricia");
    assert_eq!(person.age, None);

    let person2 = Person::builder().name("Sam").create()?;
    assert_eq!(person2.name, "Sam");
    assert_eq!(person2.age, None);
    assert_eq!(person2.id, 2);

    let deleted_id = person.id;
    let remaining_id = person2.id;
    person.delete()?;

    assert!(matches!(Person::find(deleted_id), Err(Error::Sqlite(_))));

    let remaining = Person::all()?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, remaining_id);
    assert_eq!(remaining[0].name, "Sam");
    assert_eq!(remaining[0].age, None);

    Ok(())
}
