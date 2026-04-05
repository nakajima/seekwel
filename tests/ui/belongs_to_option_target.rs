use seekwel::BelongsTo;

#[seekwel::model]
struct Person {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Pet {
    id: u64,
    owner: BelongsTo<Option<Person>>,
}

fn main() {}
