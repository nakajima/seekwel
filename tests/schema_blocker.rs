use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::schema::{PlanBlocker, SchemaBuilder};

mod target {
    #[seekwel::model]
    pub struct Thing {
        pub id: u64,
    }
}

#[test]
fn schema_plan_blocks_rebuild_when_actual_table_has_foreign_keys() -> Result<(), Error> {
    Connection::memory()?;

    Connection::get()?.execute(
        "CREATE TABLE thing (\
            id INTEGER PRIMARY KEY, \
            parent_id INTEGER, \
            FOREIGN KEY(parent_id) REFERENCES thing(id)\
         )",
        (),
    )?;

    let plan = SchemaBuilder::new().model::<target::Thing>().plan()?;

    assert!(
        plan.is_blocked(),
        "rebuild that drops parent_id should be blocked by the existing FK",
    );
    assert!(
        plan.blockers
            .iter()
            .any(|blocker| matches!(blocker, PlanBlocker::RealForeignKeys { table } if table == "thing")),
        "expected RealForeignKeys blocker for `thing`, got: {:?}",
        plan.blockers,
    );

    Ok(())
}
