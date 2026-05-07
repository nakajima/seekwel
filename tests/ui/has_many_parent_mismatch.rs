use seekwel::{BelongsTo, HasMany};

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Shelter {
    id: u64,
    pets: HasMany<Pet, { PetColumns::OWNER_ID }>,
}

#[seekwel::model]
struct Pet {
    id: u64,
    owner: BelongsTo<Person>,
}

fn main() {}
