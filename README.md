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
use seekwel::connection::Connection;

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
let person = draft.save()?; // => Person<Persisted>

// Or build + persist in one step.
let person2 = Person::builder().name("Sam").create()?;

// Persisted records can be reloaded.
let reloaded = person.reload()?;
assert_eq!(reloaded.id, person.id);
assert_eq!(person2.id, 2);
```

