# seekwel

It's a sqlite library for rust. Batteries included, but they are not replaceable.

## connections

```rs
// Initialize an in memory db to use globally
Connection::memory()?;

// Initialize a file db to use globally
Connection::file("db.sqlite")?

// Get the connection. Blows up if not initialized
Connection::get();
```

## models

```rs
#[seekwel::model]
struct Person {
  id: u64,          // All models must have an `id: u64` field
  name: String,     // Non-null TEXT column
  age: Option<u8>,  // Nullable INTEGER column
}

Connection::memory()?;
Person::create_table()?;

let draft = Person::builder()
  .name("Pat")
  .age(Some(123))
  .build()?;          // => Person<seekwel::NewRecord>

let person = draft.save()?; // => Person (aka Person<seekwel::Persisted>)
let person = person.reload()?;

let person2 = Person::builder().name("Sam").create()?;
assert_eq!(person2.id, 2);
```

## migrate

```rs
seekwel::migrate!(Person, Pet)
```


