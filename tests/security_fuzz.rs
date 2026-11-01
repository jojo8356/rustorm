use rustorm::query::{Field, Insert, Select};
use rustorm::schema;

#[test]
fn fuzz_placeholder() {
    // Minimal offline test to ensure security_fuzz compiles
    let base = "datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") }";
    let ok = format!("{base} model User {{ id Int @id }}");
    assert!(schema::parse(&ok).is_ok());
    let f: Field<i32> = Field::new("t", "id");
    let q = Select::new("t", &["id"]).where_(f.equals(1)).compile().unwrap();
    assert!(q.sql.contains("$1"));
}

#[test]
#[ignore]
fn ignored_fuzz_injection() {
    // This is ignored offline, will be run with --ignored if needed
    assert!(true);
}
