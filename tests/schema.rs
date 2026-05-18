use seekwel::connection::Connection;
use seekwel::error::Error;
use seekwel::prelude::*;
use seekwel::schema::{ApplyMode, PlanBlocker, PlanOp, SchemaBuilder};

mod v1 {
    #[seekwel::model]
    pub struct Person {
        pub id: u64,
        pub name: String,
    }
}

mod required_column {
    #[seekwel::model]
    pub struct Person {
        pub id: u64,
        pub name: String,
        pub age: u8,
    }
}

mod default_required_column {
    #[seekwel::model]
    pub struct Person {
        pub id: u64,
        pub name: String,
        #[default = false]
        pub archived: bool,
    }
}

mod v2 {
    #[seekwel::model]
    pub struct Person {
        pub id: u64,
        pub name: String,
        #[default = false]
        pub archived: bool,
        pub age: Option<u8>,
    }
}

mod v3 {
    #[seekwel::model]
    pub struct Person {
        pub id: u64,
    }
}

#[test]
fn schema_builder_plans_and_applies_safe_and_destructive_changes() -> Result<(), Error> {
    Connection::memory()?;

    let create_plan = SchemaBuilder::new().model::<v1::Person>().plan()?;
    assert!(create_plan.blockers.is_empty());
    assert!(matches!(
        create_plan.ops.as_slice(),
        [PlanOp::CreateTable { table }] if table.name == "person"
    ));
    create_plan.apply(ApplyMode::SafeOnly)?;

    let created = v1::Person::builder().name("Pat").create()?;

    let blocked_plan = SchemaBuilder::new()
        .model::<required_column::Person>()
        .plan()?;
    assert!(blocked_plan.is_blocked());
    assert!(matches!(
        blocked_plan.blockers.as_slice(),
        [PlanBlocker::RequiredColumnAddition { table, column }]
            if table == "person" && column == "age"
    ));

    let default_plan = SchemaBuilder::new()
        .model::<default_required_column::Person>()
        .plan()?;
    assert!(default_plan.blockers.is_empty());
    assert!(default_plan.ops.iter().any(|op| {
        matches!(
            op,
            PlanOp::AddColumn { table, column }
                if table == "person" && column.name == "archived" && column.default_sql.as_deref() == Some("0")
        )
    }));
    default_plan.apply(ApplyMode::SafeOnly)?;

    let after_default = default_required_column::Person::find(created.id)?;
    assert!(!after_default.archived);

    let add_plan = SchemaBuilder::new().model::<v2::Person>().plan()?;
    assert!(add_plan.blockers.is_empty());
    assert!(add_plan.ops.iter().any(|op| {
        matches!(
            op,
            PlanOp::AddColumn { table, column }
                if table == "person" && column.name == "age"
        )
    }));
    add_plan.apply(ApplyMode::SafeOnly)?;

    let after_add = v2::Person::find(created.id)?;
    assert_eq!(after_add.age, None);
    assert!(!after_add.archived);

    let rebuild_plan = SchemaBuilder::new().model::<v3::Person>().plan()?;
    assert!(rebuild_plan.blockers.is_empty());
    assert!(
        rebuild_plan
            .ops
            .iter()
            .any(|op| matches!(op, PlanOp::RebuildTable { table, .. } if table == "person"))
    );
    assert!(matches!(
        rebuild_plan.apply(ApplyMode::SafeOnly),
        Err(Error::SchemaBlocked(_))
    ));
    rebuild_plan.apply(ApplyMode::AllowDestructive)?;

    let after_rebuild = v3::Person::find(created.id)?;
    assert_eq!(after_rebuild.id, created.id);

    let drop_plan = SchemaBuilder::new().plan()?;
    assert!(drop_plan.blockers.is_empty());
    assert!(
        drop_plan
            .ops
            .iter()
            .any(|op| matches!(op, PlanOp::DropTable { table } if table.name == "person"))
    );
    drop_plan.apply(ApplyMode::AllowDestructive)?;

    let final_plan = SchemaBuilder::new().plan()?;
    assert!(final_plan.ops.is_empty());
    assert!(final_plan.blockers.is_empty());

    Ok(())
}
