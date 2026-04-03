# seekwel

It's a sqlite library for Rust. Batteries included, but they are not replaceable.

## connection

The connection is global. If you don't like globals, too bad. this is the only way to use sqlite in rust so you're plum out of luck.

```rs
use seekwel::connection::Connection;

// Initialize an in-memory db to use globally
Connection::memory()?;

// Or initialize a file db to use globally
Connection::file("db.sqlite")?;

// Get the global connection if you want. I'm not sure why you would.
let conn = Connection::get()?;
```

## models

Everything you might want to save in the db is a "Model". There's a `model` macro that sets everything up all nice.

```rs
use seekwel::{Comparison, connection::Connection, prelude::*};

#[seekwel::model]
struct Person {
    id: u64,          // All models must have an `id: u64` field
    name: String,     // Non-null TEXT column
    age: Option<u8>,  // Nullable INTEGER column
}

Connection::memory()?;
Person::create_table()?;

// Build an unsaved record in memory.
let draft = Person::builder()
    .name("Pat")
    .age(Some(123))
    .build()?; // => Person<NewRecord>

// Persist it.
let mut person = draft.save()?; // => Person<Persisted>

// Or build + persist in one step.
let person2 = Person::builder().name("Sam").create()?;

// Persisted records can be reloaded.
person.reload()?;

// Query builder methods are provided by the prelude.
// The model macro also generates a `PersonColumns` enum for type-safe queries.
// Persisted records can be queried.
let person = Person::find(1)?; // => Person<Persisted>
let people = Person::all()?; // => Vec<Person<Persisted>>
let person = Person::first()?; // => Option<Person<Persisted>>
let person = Person::q(PersonColumns::Name, Comparison::Eq("Pat")).first()?; // => Option<Person<Persisted>>
let person = Person::q(PersonColumns::Name, Comparison::Ne("Pat")).first()?; // => Option<Person<Persisted>>
let people = Person::q(PersonColumns::Age, Comparison::Gte(21)).all()?; // => Vec<Person<Persisted>>

// q(...) returns a query builder. Use first(), all(), iter(), or try_iter() to execute it.
let people = Person::q(PersonColumns::Age, Comparison::Gte(21))
    .and(Person::q(PersonColumns::Name, Comparison::Eq("Pat")))
    .all()?;

// Chaining q(...) on a query adds another AND clause.
let people = Person::q(PersonColumns::Age, Comparison::Gte(21))
    .q(PersonColumns::Name, Comparison::Eq("Pat"))
    .all()?;

// You can also group OR clauses.
let people = Person::q(PersonColumns::Age, Comparison::Gte(21))
    .and(Person::q(PersonColumns::Name, Comparison::Eq("Pat")).or(Person::q(PersonColumns::Name, Comparison::Eq("Sam"))))
    .all()?;

// You can also iterate over query results.
for person in Person::q(PersonColumns::Age, Comparison::Gte(21)).iter()? {
    println!("{}", person.name);
}

// Or iterate directly. This uses the plain/panicking path.
for person in Person::q(PersonColumns::Age, Comparison::Gte(21)) {
    println!("{}", person.name);
}

// Eager try_iter() still yields plain records.
for person in Person::q(PersonColumns::Age, Comparison::Gte(21)).try_iter()? {
    println!("{}", person.name);
}

// Fetch strategy modifiers also work directly from the model type.
for person in Person::lazy().iter()? {
    println!("{}", person.name);
}

for person in Person::q(PersonColumns::Age, Comparison::Gte(21)).lazy().iter()? {
    println!("{}", person.name);
}

// Lazy try_iter() yields Result items.
for person in Person::q(PersonColumns::Age, Comparison::Gte(21)).lazy().try_iter()? {
    let person = person?;
    println!("{}", person.name);
}

// chunked(n) yields chunks.
for people in Person::q(PersonColumns::Age, Comparison::Gte(21)).chunked(100) {
    for person in people {
        println!("{}", person.name);
    }
}

// first() and all() stay eager even on lazy/chunked queries.
let people = Person::q(PersonColumns::Age, Comparison::Gte(21)).lazy().all()?;

// Eq(None::<T>) becomes IS NULL. Ne(None::<T>) becomes IS NOT NULL.
let people = Person::q(PersonColumns::Age, Comparison::Eq(None::<u8>)).all()?;

// Comparison supports Eq, Ne, Gt, Gte, Lt, and Lte.
```

Custom field types can implement `seekwel::SqlField` to control how they are stored, loaded, and queried.
