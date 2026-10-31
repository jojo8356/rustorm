use rustorm::query::{Field, Filter, Select, Value};

// Helper to create a simple Select for table "users" with columns id, name, email
fn users_select() -> Select {
    Select::new("users", &["id", "name", "email"])
}
fn field<T>(col: &'static str) -> Field<T> { Field::new("users", col) }

#[test]
fn empty_in_is_false_not_in_is_true() {
    let f = field::<i32>("id");
    // in([]) should render to 1=0, not error
    let q_empty_in = users_select().where_(f.is_in(Vec::<i32>::new())).compile().unwrap();
    assert!(q_empty_in.sql.contains("1=0"), "sql: {}", q_empty_in.sql);
    assert!(q_empty_in.params.is_empty());

    let q_empty_not_in = users_select().where_(f.not_in(Vec::<i32>::new())).compile().unwrap();
    assert!(q_empty_not_in.sql.contains("1=1"), "sql: {}", q_empty_not_in.sql);
}

#[test]
fn empty_in_via_some_variant() {
    let f = field::<Option<String>>("email");
    let q = users_select().where_(f.in_some(Vec::<String>::new())).compile().unwrap();
    assert!(q.sql.contains("1=0"), "sql: {}", q.sql);
    let q2 = users_select().where_(f.not_in_some(Vec::<String>::new())).compile().unwrap();
    assert!(q2.sql.contains("1=1"), "sql: {}", q2.sql);
}

#[test]
fn find_unique_helper_generates_select() {
    // find_unique is currently alias to find_many with filter; test that we can build unique filter
    let id = field::<i32>("id");
    let sel = users_select().where_(id.equals(1)).compile().unwrap();
    assert!(sel.sql.contains("WHERE"), "sql: {}", sel.sql);
    assert!(sel.sql.contains("\"users\""), "sql: {}", sel.sql);
    // The new generated helper find_unique() should exist and compile similarly
    // We test via Select::new directly as generated code does
    let uniq = users_select().where_(id.equals(1)).take(1).compile().unwrap();
    assert!(uniq.sql.contains("LIMIT"), "sql: {}", uniq.sql);
}

#[test]
fn find_first_with_order() {
    let name = field::<String>("name");
    let sel = users_select().order_by(name.asc()).take(1).compile().unwrap();
    assert!(sel.sql.contains("ORDER BY"), "sql: {}", sel.sql);
    assert!(sel.sql.contains("LIMIT"), "sql: {}", sel.sql);
}

#[test]
fn cursor_pagination_adds_filter() {
    let id = field::<i32>("id");
    let sel = users_select().cursor(id, 10).order_by(id.asc()).take(5).compile().unwrap();
    assert!(sel.sql.contains("WHERE"), "sql: {}", sel.sql);
    assert!(sel.sql.contains("> $"), "sql: {}", sel.sql);
    assert!(sel.sql.contains("ORDER BY"), "sql: {}", sel.sql);
    assert!(sel.sql.contains("LIMIT"), "sql: {}", sel.sql);
    assert_eq!(sel.params.len(), 2); // cursor value + limit
    assert_eq!(sel.params[0], Value::Int(10));
}

#[test]
fn distinct_multi_column_requires_selection() {
    // distinct requires selected fields to include order fields
    let name = field::<String>("name");
    let id = field::<i32>("id");
    // select only name but order by id should error
    let sel = users_select().select(name).distinct().order_by(id.asc());
    assert!(sel.compile().is_err());
    // correct: select both, order by one of them
    let sel2 = users_select().select(name).select(id).distinct().order_by(id.asc()).compile().unwrap();
    assert!(sel2.sql.contains("DISTINCT"), "sql: {}", sel2.sql);
}

#[test]
fn select_projection_single_field() {
    let name = field::<String>("name");
    let sel = users_select().select(name).compile().unwrap();
    assert!(sel.sql.contains("\"name\""), "sql: {}", sel.sql);
    assert!(!sel.sql.contains("\"email\""), "sql: {}", sel.sql);
    // multi select
    let id = field::<i32>("id");
    let sel2 = users_select().select(name).select(id).compile().unwrap();
    assert!(sel2.sql.contains("\"name\"") && sel2.sql.contains("\"id\""), "sql: {}", sel2.sql);
}

#[test]
fn pagination_bounds_checked() {
    let sel = users_select().skip(u64::MAX).compile();
    assert!(sel.is_err());
    let sel2 = users_select().take(i64::MAX as u64 + 1).compile();
    assert!(sel2.is_err());
}

#[test]
fn not_equals_and_in_variants() {
    let email = field::<String>("email");
    let q = users_select().where_(email.not_equals("a@b.com")).compile().unwrap();
    assert!(q.sql.contains("<>"), "sql: {}", q.sql);
    let q2 = users_select().where_(email.is_in(vec!["a", "b"])).compile().unwrap();
    assert!(q2.sql.contains("IN"), "sql: {}", q2.sql);
}

#[test]
fn insert_many_stub_errors_as_p1() {
    let stub = rustorm::query::InsertManyStub::new("users");
    assert!(stub.compile().is_err());
}
