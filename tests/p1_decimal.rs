use rustorm::schema::{parse, format, Scalar};

#[test]
fn decimal_with_precision_scale_parsed() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model Product { id Int @id price Decimal @db.Decimal(10,2) }"#;
    let schema = parse(src).expect("parse decimal");
    let field = &schema.models[0].fields.iter().find(|f| f.name=="price").unwrap();
    assert!(matches!(field.ty, Scalar::Decimal{precision: Some(10), scale: Some(2)}));
}

#[test]
fn decimal_without_precision_defaults() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model Invoice { id Int @id amount Decimal }"#;
    let schema = parse(src).unwrap();
    let field = &schema.models[0].fields.iter().find(|f| f.name=="amount").unwrap();
    assert!(matches!(field.ty, Scalar::Decimal{precision: None, scale: None}));
}

#[test]
fn decimal_scale_exceeds_precision_rejected() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model Bad { id Int @id price Decimal @db.Decimal(2,10) }"#;
    assert!(parse(src).is_err());
}

#[test]
fn decimal_ddl_numeric() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model Product { id Int @id price Decimal @db.Decimal(10,2) }"#;
    let schema = parse(src).unwrap();
    let sql = rustorm::migrations::create_sql(&schema).unwrap();
    assert!(sql.contains("NUMERIC(10,2)"), "ddl: {sql}");
}

#[test]
fn decimal_format_roundtrip() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model P { id Int @id price Decimal @db.Decimal(10,2) }"#;
    let formatted = format(src).unwrap();
    assert!(formatted.contains("Decimal") && formatted.contains("@db.Decimal(10,2)"), "{formatted}");
    let reparsed = parse(&formatted).unwrap();
    assert_eq!(reparsed.models.len(), 1);
}
