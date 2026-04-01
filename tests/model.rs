use seekwel::connection::Connection;
use seekwel::error::Error;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
    age: Option<u8>,
}

fn expect_new_record(_: &Person<seekwel::NewRecord>) {}
fn expect_persisted(_: &Person) {}

#[test]
fn builder_save_and_reload() -> Result<(), Error> {
    Connection::memory()?;
    Person::create_table()?;

    let draft = Person::builder().name("Pat").age(Some(100)).build()?;
    expect_new_record(&draft);
    assert_eq!(draft.name, "Pat");
    assert_eq!(draft.age, Some(100));
    assert_eq!(draft.id, 0);

    let person = draft.save()?;
    expect_persisted(&person);
    assert_eq!(person.id, 1);

    let id = person.id;
    let reloaded = person.reload()?;
    assert_eq!(reloaded.id, id);
    assert_eq!(reloaded.name, "Pat");
    assert_eq!(reloaded.age, Some(100));

    let person2 = Person::builder().name("Sam").create()?;
    assert_eq!(person2.name, "Sam");
    assert_eq!(person2.age, None);
    assert_eq!(person2.id, 2);

    Ok(())
}
