use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::schema::{PlanOp, SchemaBuilder};

#[seekwel::model]
struct Cat {
    id: u64,
    name: String,
}

#[seekwel::model]
struct Dog {
    id: u64,
    age: Option<u8>,
}

#[test]
fn registered_schema_builder_discovers_models() -> Result<(), Error> {
    let schema = SchemaBuilder::registered()?.build()?;
    let table_names = schema
        .tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(table_names, vec!["cat", "dog"]);
    assert_eq!(schema.tables[0].columns[0].name, "name");
    assert_eq!(schema.tables[1].columns[0].name, "age");
    assert!(schema.tables[1].columns[0].nullable);
    Ok(())
}

#[test]
fn registered_schema_builder_can_plan_against_live_db() -> Result<(), Error> {
    Connection::memory()?;
    let plan = SchemaBuilder::registered()?.plan()?;

    assert_eq!(plan.blockers.len(), 0);
    assert_eq!(plan.ops.len(), 2);
    assert!(matches!(plan.ops[0], PlanOp::CreateTable { .. }));
    assert!(matches!(plan.ops[1], PlanOp::CreateTable { .. }));
    Ok(())
}
