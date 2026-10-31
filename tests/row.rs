use rustorm::row::{Column, DecodeError, Row};
fn col(name: &str, oid: u32, value: Option<&str>) -> Column {
    Column {
        name: name.into(),
        oid,
        value: value.map(str::to_owned),
    }
}
#[test]
fn decodes_values_and_nulls_by_type_and_oid() {
    let row = Row::new(vec![
        col("id", 23, Some("42")),
        col("name", 25, None),
        col("enabled", 16, Some("t")),
        col("score", 701, Some("1.25")),
        col("big", 20, Some("10000000000")),
    ]);
    assert_eq!(row.get::<i32>("id").unwrap(), 42);
    assert_eq!(row.get::<Option<String>>("name").unwrap(), None);
    assert!(row.get::<bool>("enabled").unwrap());
    assert_eq!(row.get::<f64>("score").unwrap(), 1.25);
    assert_eq!(row.get::<i64>("big").unwrap(), 10_000_000_000);
    assert_eq!(
        row.get::<String>("name").unwrap_err(),
        DecodeError::UnexpectedNull
    );
    assert!(matches!(
        row.get::<Option<i32>>("name"),
        Err(DecodeError::UnexpectedType { .. })
    ));
    assert!(matches!(
        row.get::<i64>("id"),
        Err(DecodeError::UnexpectedType { .. })
    ));
    assert!(matches!(
        row.get::<i32>("missing"),
        Err(DecodeError::MissingColumn(_))
    ));
}
#[test]
fn duplicate_invalid_values_and_debug_redaction() {
    let row = Row::new(vec![
        col("email", 25, Some("secret@example.org")),
        col("email", 25, Some("other")),
        col("bad", 23, Some("invalid")),
    ]);
    assert!(matches!(
        row.get::<String>("email"),
        Err(DecodeError::AmbiguousColumn(_))
    ));
    assert!(matches!(
        row.get::<i32>("bad"),
        Err(DecodeError::InvalidValue(_))
    ));
    assert!(!format!("{row:?}").contains("secret@example.org"));
}
