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

### define a model

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
```

### create and persist records

```rs
// Build an unsaved record in memory.
let draft = Person::builder()
    .name("Pat")
    .age(Some(123))
    .build()?; // => Person<NewRecord>

// Persist it.
let mut person = draft.save()?; // => Person<Persisted>

// Or build + persist in one step.
let person2 = Person::builder().name("Sam").create()?;

// Persisted records can be saved again after local changes, then reloaded.
person.age = Some(124);
person.save()?;
person.reload()?;

// Persisted records can also be deleted.
let delete_me = Person::builder().name("Delete Me").create()?;
delete_me.delete()?;
```

### query records

The model macro also generates a `PersonColumns` enum for type-safe queries.

```rs
let person = Person::find(1)?; // => Person<Persisted>
let people = Person::all()?; // => Vec<Person<Persisted>>
let person = Person::first()?; // => Option<Person<Persisted>>

let person = Person::q(PersonColumns::Name, Comparison::Eq("Pat")).first()?;
let person = Person::q(PersonColumns::Name, Comparison::Ne("Pat")).first()?;
let people = Person::q(PersonColumns::Age, Comparison::Gte(21)).all()?;

let count = Person::q(PersonColumns::Age, Comparison::Gte(21)).count()?;
let exists = Person::q(PersonColumns::Name, Comparison::Eq("Pat")).exists()?;
```

### combine filters

```rs
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
    .and(
        Person::q(PersonColumns::Name, Comparison::Eq("Pat"))
            .or(Person::q(PersonColumns::Name, Comparison::Eq("Sam"))),
    )
    .all()?;
```

### ordering and pagination

```rs
let people = Person::order(PersonColumns::Name).limit(10).offset(20).all()?;
let people = Person::order(PersonColumns::Name.desc()).all()?;
let people = Person::order([PersonColumns::Age.desc(), PersonColumns::Name.asc()]).all()?;
let people = Person::order("name DESC").all()?;

let people = Person::q(PersonColumns::Age, Comparison::Gte(21))
    .order(PersonColumns::Name.desc())
    .limit(5)
    .all()?;
```

### iterate results

```rs
// You can iterate over query results.
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
```

### fetch strategies

```rs
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
```

### notes

- Prefer `Comparison::IsNull` and `Comparison::IsNotNull` for null checks.
- `Comparison::Eq(None::<T>)` still becomes `IS NULL`.
- `Comparison::Ne(None::<T>)` still becomes `IS NOT NULL`.
- `Comparison` supports `Eq`, `Ne`, `Gt`, `Gte`, `Lt`, `Lte`, `IsNull`, and `IsNotNull`.

## custom field types

Custom field types can implement `seekwel::SqlField` to control how they are stored, loaded, and queried.

## belongs_to relations

First-pass `belongs_to` relations are supported with `BelongsTo<T>` and `Option<BelongsTo<T>>`.

```rs
use seekwel::{BelongsTo, connection::Connection, prelude::*};

#[seekwel::model]
#[derive(Clone)]
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

# fn main() -> Result<(), Box<dyn std::error::Error>> {
Connection::memory()?;
Person::create_table()?;
Pet::create_table()?;

let pat = Person::builder().name("Pat").create()?;
let pet = Pet::builder().name("Fido").owner(pat.clone()).create()?;

// Load the parent record. Results are cached on the relation field.
let owner = pet.owner()?;
assert_eq!(owner.name, "Pat");

// Relation fields are stored as `<field>_id` columns for querying.
let pets = Pet::q(PetColumns::OwnerId, seekwel::Comparison::Eq(pat.id)).all()?;
assert_eq!(pets.len(), 1);
# Ok(())
# }
```

Relation loaders clone from the cached parent value in this first pass, so target models should implement `Clone` (with `#[derive(Clone)]` placed below `#[seekwel::model]`).

`BelongsTo<Option<T>>` is not supported; use `Option<BelongsTo<T>>` instead.
