use rusqlite::types::Value;
use seekwel::Comparison;
use seekwel::SqlField;
use seekwel::connection::Connection;
use seekwel::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Email(String);

impl SqlField for Email {
    const SQL_TYPE: &'static str = "TEXT";

    fn to_sql_value(&self) -> Value {
        Value::Text(self.0.clone())
    }

    fn from_sql_row(row: &rusqlite::Row, index: usize) -> rusqlite::Result<Self> {
        Ok(Self(row.get(index)?))
    }
}

#[seekwel::model]
struct Contact {
    id: u64,
    email: Email,
    backup_email: Option<Email>,
}

#[test]
fn user_defined_sql_field_round_trips() -> Result<(), Error> {
    Connection::memory()?;
    Contact::create_table()?;

    let pat = Contact::builder()
        .email(Email("pat@example.com".to_string()))
        .backup_email(Some(Email("backup@example.com".to_string())))
        .create()?;

    let alex = Contact::builder()
        .email(Email("alex@example.com".to_string()))
        .create()?;

    let found = Contact::find(pat.id)?;
    assert_eq!(found.email, Email("pat@example.com".to_string()));
    assert_eq!(
        found.backup_email,
        Some(Email("backup@example.com".to_string()))
    );

    let by_email = Contact::q(
        "email",
        Comparison::Eq(Email("alex@example.com".to_string())),
    )
    .first()?;
    assert_eq!(by_email.map(|contact| contact.id), Some(alex.id));

    let null_backup = Contact::q("backup_email", Comparison::Eq(None::<Email>)).first()?;
    assert_eq!(null_backup.map(|contact| contact.id), Some(alex.id));

    Ok(())
}
