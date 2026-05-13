use seekwel::{BelongsTo, HasMany};

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Shelter {
    id: u64,
    #[key = owner_id]
    pets: HasMany<Pet>,
}

#[seekwel::model]
struct Pet {
    id: u64,
    owner: BelongsTo<Person>,
}

fn main() {}
