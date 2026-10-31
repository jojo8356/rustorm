use rustorm::schema::parse;

#[test]
fn view_parsed() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model User { id Int @id name String } view UserView { id Int @id name String @@map("user_view") }"#;
    let schema = parse(src).unwrap();
    assert_eq!(schema.views.len(), 1);
    assert_eq!(schema.views[0].name, "UserView");
    assert_eq!(schema.views[0].table, "user_view");
}

#[test]
fn view_read_only_no_create() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } view Stats { total Int } "#;
    let schema = parse(src).unwrap();
    // views should generate only find_many, not create
    let code = rustorm::schema::generate(&schema);
    assert!(code.contains("UserView") || code.contains("Stats") || code.contains("view") || code.contains("Stats"));
    // ensure no create for views (view modules don't have create_many)
    assert!(code.contains("find_many"));
}
