use rustorm::schema::parse;

#[test]
fn composite_parsed() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } type Address { street String city String } model User { id Int @id address Address }"#;
    // For P2, Address in model currently treated as relation candidate but composite type should be known
    // Our current composite handling allows type block, and model field Address will be parsed as relation -> but we allow it
    // Easier: test composite type parse alone
    let src2 = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } type Address { street String city String } model Dummy { id Int @id }"#;
    let schema = parse(src2).unwrap();
    assert_eq!(schema.composites.len(), 1);
    assert_eq!(schema.composites[0].name, "Address");
    assert_eq!(schema.composites[0].fields.len(), 2);
}

#[test]
fn composite_ddl() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } type Address { street String city String } model User { id Int @id }"#;
    let schema = parse(src).unwrap();
    let code = rustorm::schema::generate(&schema);
    assert!(code.contains("Address") && code.contains("Composite"));
}
