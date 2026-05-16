use std::sync::{Mutex, OnceLock};

use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;
use seekwel::schema::SchemaBuilder;

#[seekwel::model(table_name = "indexed_person")]
struct IndexedPerson {
    id: u64,
    #[index]
    name: String,
    #[unique]
    email: String,
}

static INDEX_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_index_schema(test: impl FnOnce() -> Result<(), Error>) -> Result<(), Error> {
    let _guard = INDEX_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    match Connection::memory() {
        Ok(()) | Err(Error::AlreadyInitialized) => {}
        Err(err) => return Err(err),
    }

    Connection::get()?.execute("DROP TABLE IF EXISTS indexed_person", ())?;

    test()
}

#[test]
fn index_attributes_create_indexes_and_uniques() -> Result<(), Error> {
    with_index_schema(|| {
        let schema = SchemaBuilder::new().model::<IndexedPerson>().build()?;
        assert_eq!(schema.tables[0].indexes.len(), 2);
        assert!(schema.tables[0].indexes.iter().any(|index| {
            index.name == "seekwel_idx_indexed_person_name"
                && index.column == "name"
                && !index.unique
        }));
        assert!(schema.tables[0].indexes.iter().any(|index| {
            index.name == "seekwel_idx_indexed_person_email"
                && index.column == "email"
                && index.unique
        }));

        IndexedPerson::create_table()?;
        let index_sql = indexed_person_index_sql()?;
        assert!(index_sql.iter().any(|sql| {
            sql == "CREATE INDEX seekwel_idx_indexed_person_name ON indexed_person (name)"
        }));
        assert!(index_sql.iter().any(|sql| {
            sql == "CREATE UNIQUE INDEX seekwel_idx_indexed_person_email ON indexed_person (email)"
        }));

        IndexedPerson::builder()
            .name("Pat")
            .email("pat@example.com")
            .create()?;
        assert!(
            IndexedPerson::builder()
                .name("Other")
                .email("pat@example.com")
                .create()
                .is_err(),
            "#[unique] should create a uniqueness constraint"
        );

        Ok(())
    })
}

fn indexed_person_index_sql() -> Result<Vec<String>, Error> {
    Connection::get()?.query_all(
        "SELECT sql FROM sqlite_schema \
         WHERE type = 'index' AND tbl_name = 'indexed_person' AND sql IS NOT NULL \
         ORDER BY name",
        (),
        |row| row.get::<_, String>(0),
    )
}
