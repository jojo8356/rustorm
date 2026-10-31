use rustorm::query::{Field, StringQueryMode};

#[test]
fn json_equals_generates_jsonb_equals() {
    let data: Field<String> = Field { table: "users", name: "data", marker: std::marker::PhantomData };
    let f = data.json_equals(r#"{"a":1}"#);
    let q = rustorm::query::Select::new("users", &["id"]).where_(f).compile().unwrap();
    assert!(q.sql.contains("::jsonb"), "{}", q.sql);
}

#[test]
fn json_contains_generates_at_gt() {
    let data: Field<String> = Field { table: "users", name: "data", marker: std::marker::PhantomData };
    let f = data.json_contains(r#"{"a":1}"#);
    let q = rustorm::query::Select::new("users", &["id"]).where_(f).compile().unwrap();
    assert!(q.sql.contains("@>"), "{}", q.sql);
}

#[test]
fn json_path_equals_generates_hash_gt_gt() {
    let data: Field<String> = Field { table: "users", name: "data", marker: std::marker::PhantomData };
    let f = data.json_path_equals(&["a","b"], "value");
    let q = rustorm::query::Select::new("users", &["id"]).where_(f).compile().unwrap();
    assert!(q.sql.contains("#>>"), "{}", q.sql);
}

#[test]
fn string_query_mode_insensitive_uses_ilike() {
    let name: Field<String> = Field { table: "users", name: "name", marker: std::marker::PhantomData };
    let f = name.contains_with_mode("test", StringQueryMode::Insensitive);
    let q = rustorm::query::Select::new("users", &["id"]).where_(f).compile().unwrap();
    assert!(q.sql.contains("ILIKE"), "{}", q.sql);
}

#[test]
fn bytes_contains() {
    let file: Field<Vec<u8>> = Field { table: "files", name: "data", marker: std::marker::PhantomData };
    let f = file.equals(vec![1,2,3]);
    let q = rustorm::query::Select::new("files", &["id"]).where_(f).compile().unwrap();
    assert!(q.sql.contains("="), "{}", q.sql);
}
