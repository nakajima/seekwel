# seekwel

It's a sqlite library for Rust. Batteries included, but they are not replaceable.

## connection

The connection manager is global. If you don't like globals, too bad. this is the only way to use sqlite in rust so you're plum out of luck.

```rs
use seekwel::connection::Connection;

// Initialize an in-memory db to use globally. This uses one SQLite connection.
Connection::memory()?;

// Or initialize a file db to use globally. This uses WAL plus a small reader pool.
Connection::file("db.sqlite")?;

// Get a lightweight handle if you want. I'm not sure why you would.
let conn = Connection::get()?;
```

File databases use one writer connection and a small pool of read-only connections. Reads can run concurrently with other reads, and in WAL mode they can continue while a write transaction is open. Writes still serialize because SQLite only has one writer.

Transactions are implicit on the current thread:

```rs
Connection::transaction(|| {
    let pat = Person::builder().name("Pat").create()?;
    pat.save()?;
    assert_eq!(Person::count()?, 1);
    Ok(())
})?;
```

Nested transactions use savepoints. Returning an error rolls the current transaction back; `Connection::rollback()` is a convenience error for intentional rollback.

Async apps can enable the optional `tokio` feature:

```toml
seekwel = { version = "0.1.15", features = ["serde", "tokio"] }
```

With that feature, seekwel uses `tokio::task::block_in_place` for database work when called from a multi-threaded Tokio runtime. The API stays synchronous, but Tokio is told that the SQLite section may block.

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

// Find by selected builder fields, then update the row or create it.
let pat = Person::builder()
    .name("Pat")
    .age(Some(123))
    .create_or_update_by([PersonColumns::Name])?;

// Persisted records can be saved again after local changes, then reloaded.
person.age = Some(124);
person.save()?;
person.reload()?;

// Persisted records can also be deleted.
let delete_me = Person::builder().name("Delete Me").create()?;
delete_me.delete()?;
```

### create and update from params

The model macro also generates a `<Model>Params` type for form-like input. Params must be filtered with `allow(...)` before they can assign model fields.

```rs
let person = Person::create(
    PersonParams::new()
        .name("Pat")
        .age(Some(123))
        .allow([PersonColumns::Name, PersonColumns::Age]),
)?;

let mut person = Person::find(person.id)?;
person.update(
    PersonParams::new()
        .name("Patricia")
        .allow([PersonColumns::Name]),
)?;
```

With the `serde` feature enabled, params objects implement `serde::Deserialize`, so they can be used with Axum forms without adding an Axum dependency to seekwel.

```rs
async fn create_person(
    axum::extract::Form(params): axum::extract::Form<PersonParams>,
) -> Result<(), AppError> {
    Person::create(params.allow([PersonColumns::Name, PersonColumns::Age]))?;
    Ok(())
}
```

Association params use their stored column name, like `owner_id` for `owner: BelongsTo<Person>`. `HasMany` fields are not included in params.

### validations

Use a validator hook when declaring a model. Failed saves return `SaveError::Invalid(model)`, where the model has an `Invalid` typestate and carries Rails-style errors.

```rs
use seekwel::prelude::*;

struct PersonValidator;

#[seekwel::model(validator = PersonValidator)]
struct Person {
    id: u64,
    name: String,
}

impl<S> seekwel::Validator<Person<S>> for PersonValidator {
    fn validate(person: &Person<S>, errors: &mut seekwel::Errors<PersonColumns>) {
        if person.name.trim().is_empty() {
            errors.add(PersonColumns::Name, "can't be blank");
        }
    }
}

let draft = Person::builder().name("").build()?;
if let Err(seekwel::SaveError::Invalid(invalid)) = draft.save() {
    assert_eq!(invalid.errors().on(PersonColumns::Name), vec!["can't be blank"]);
}
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

## associations

### belongs_to

```rs
use seekwel::{BelongsTo, connection::Connection, prelude::*};

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Pet {
    id: u64,
    name: String,
    owner: BelongsTo<Person>, // Non-null owner_id
    sitter: Option<BelongsTo<Person>>, // Nullable sitter_id
}

Connection::memory()?;
Person::create_table()?;
Pet::create_table()?;

let pat = Person::builder().name("Pat").create()?;
let pet = Pet::builder().name("Fido").owner(pat.clone()).create()?;

// Load the parent record. Results are cached on the association field.
let owner = pet.owner()?;
assert_eq!(owner.name, "Pat");

// Association fields are stored as `<field>_id` columns for querying.
let pets = Pet::q(PetColumns::OwnerId, seekwel::Comparison::Eq(pat.id)).all()?;
assert_eq!(pets.len(), 1);
```

> [!WARNING]  
> `BelongsTo<Option<T>>` is not supported; use `Option<BelongsTo<T>>` instead.

### has_many

Basically the same as above, except we add a `HasMany<Pet>` field.

> [!NOTE]
> `HasMany` uses a const-generic association key, so the field type is written as `HasMany<Pet, { PetColumns::OWNER_ID }>`.

```rs
use seekwel::{HasMany, connection::Connection, prelude::*};

#[seekwel::model]
struct Person {
    id: u64,
    name: FTS<String>,
    pets: HasMany<Pet, { PetColumns::OWNER_ID }> // Validates that Pet has an `owner: BelongsTo<Person>` field
}
Connection::memory()?;
Person::create_table()?;
Pet::create_table()?;

let owner = Person::builder().name("Pat").create()?;
owner.pets.append(Pet::builder().name("Fido"))?;
assert_eq!(owner.pets()?.len(), 1);
assert_eq!(owner.pets()?[0].owner, owner);

// Association fields are stored as `<field>_id` columns for querying.
let pets = Pet::q(PetColumns::OwnerId, seekwel::Comparison::Eq(owner.id)).all()?;
assert_eq!(pets.len(), 1);
```