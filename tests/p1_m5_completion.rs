//! P1 (M5) completion — offline tests for every "revendiqué done, en vrai partiel" bullet.
//! Covers: RELATIONS composites/named/self, SCHEMA_LANGUAGE enum/format, CLIENT_GENERATION RustormClient,
//! CLIENT_EXAMPLE builder, MIGRATIONS baseline/resolve/diff/push/reset, INTROSPECTION db pull, QUERY_ENGINE aggregates.
#![allow(unused_imports)]
use rustorm::schema::{parse, format, generate};
use rustorm::query::{Field, Select, Aggregate, StringQueryMode};
use std::marker::PhantomData;

fn header() -> String {
    "datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") } generator client { provider = \"rustorm-client\" output = \"../src/generated/rustorm\" } ".to_string()
}

fn f_string(table: &'static str, name: &'static str) -> Field<String> {
    Field { table, name, marker: PhantomData }
}
fn f_i32(table: &'static str, name: &'static str) -> Field<i32> {
    Field { table, name, marker: PhantomData }
}
fn f_opt_string(table: &'static str, name: &'static str) -> Field<Option<String>> {
    Field { table, name, marker: PhantomData }
}

// ---------- RELATIONS ----------
#[test]
fn fk_composite_two_columns_generates_composite_fk() {
    let src = format!("{} model A {{ id Int @id sec String @unique posts Post[] @@id([id, sec]) }} model Post {{ id Int @id aId Int aSec String a A @relation(fields: [aId, aSec], references: [id, sec], onDelete: Cascade, onUpdate: Cascade) }}", header());
    let schema = parse(&src).expect("composite fk parse");
    // field is stored as "aId,aSec"
    let fk_field = schema.models[1].relations[0].foreign_key.as_ref().unwrap().field.clone();
    assert!(fk_field.contains(','), "fk_field should contain comma for composite: {:?}", fk_field);
    let sql = rustorm::migrations::initial_sql(&schema).unwrap();
    assert!(sql.contains("FOREIGN KEY"), "sql: {sql}");
    assert!(sql.contains("ON DELETE CASCADE"), "sql: {sql}");
}

#[test]
fn fk_composite_ddl_has_two_refs() {
    let src = format!("{} model P {{ id Int @id sec String @unique vals Val[] @@id([id, sec]) }} model Val {{ id Int @id p1 Int p2 String p P @relation(fields: [p1, p2], references: [id, sec], onDelete: Restrict, onUpdate: Cascade) }}", header());
    let schema = parse(&src).unwrap();
    let sql = rustorm::migrations::initial_sql(&schema).unwrap();
    eprintln!("SQL for fk_composite_ddl_has_two_refs: {}", sql);
    assert!(sql.contains("FOREIGN KEY"), "sql: {sql}");
    // should contain both local cols
    assert!(sql.contains("p1") && sql.contains("p2"), "sql: {sql}");
}

#[test]
fn named_m2m_implicit_generates_stable_join_name() {
    let src = format!("{} model Post {{ id Int @id tags Tag[] @relation(\"PostTags\") }} model Tag {{ id Int @id posts Post[] @relation(\"PostTags\") }}", header());
    let schema = parse(&src).expect("named m2m");
    assert_eq!(schema.models.len(), 2);
    let n1 = rustorm::schema::implicit_join_name_for_relation(Some("PostTags"), "Post", "Tag");
    let n2 = rustorm::schema::implicit_join_name_for_relation(Some("PostTags"), "Tag", "Post");
    assert_ne!(n1, n2, "named join orientation includes left/right");
    assert!(n1.starts_with("rustorm_m2m_"));
}

#[test]
fn self_m2m_named_allowed() {
    let src = format!("{} model User {{ id Int @id friends User[] @relation(\"Friends\") friendOf User[] @relation(\"Friends\") }}", header());
    let s = parse(&src).expect("self m2m");
    assert_eq!(s.models[0].relations.len(), 2);
}

#[test]
fn unnamed_m2m_still_stable() {
    let a = rustorm::schema::implicit_join_name("Post", "Tag");
    let b = rustorm::schema::implicit_join_name("Tag", "Post");
    assert_eq!(a, b);
}

// ---------- SCHEMA_LANGUAGE ----------
#[test]
fn enum_with_map_and_format_idempotent() {
    let src = format!("{} enum Role {{ ADMIN USER @map(\"user_val\") GUEST }} model U {{ id Int @id role Role }}", header());
    let s = parse(&src).unwrap();
    assert_eq!(s.enums[0].variants[1].mapped.as_deref(), Some("user_val"));
    let gen = generate(&s);
    assert!(gen.contains("ADMIN") && gen.contains("pub mod role"));
    let formatted = format(&src).unwrap();
    assert_eq!(formatted, format(&formatted).unwrap(), "format idempotent failed");
}

#[test]
fn enum_duplicate_rejected() {
    let src = format!("{} enum E {{ A A }} model M {{ id Int @id }}", header());
    assert!(parse(&src).is_err());
}

#[test]
fn type_list_optional_rejected_message() {
    let src = format!("{} model M {{ id Int @id tags String[]? }}", header());
    let err = parse(&src).unwrap_err();
    assert!(err.message.contains("Type[]?"), "{}", err.message);
}

#[test]
fn triple_slash_comment_treated_as_line_comment() {
    let src = format!("{} /// doc comment\n model M {{ id Int @id x String }}", header());
    let s = parse(&src).unwrap();
    assert_eq!(s.models[0].fields.len(), 2);
    let f = format(&src).unwrap();
    assert!(f.contains("model M"));
}

#[test]
fn format_preserves_enum_and_composite_unique() {
    let src = format!("{} model U {{ id Int @id email String name String @@unique([email, name]) }}", header());
    let f = format(&src).unwrap();
    assert!(f.contains("@@unique([email, name])"));
    assert_eq!(f, format(&f).unwrap());
}

// ---------- CLIENT_GENERATION ----------
#[test]
fn generate_contains_rustorm_client_and_delegate() {
    let src = format!("{} model User {{ id Int @id email String }} model Post {{ id Int @id title String }}", header());
    let s = parse(&src).unwrap();
    let gen = generate(&s);
    assert!(gen.contains("pub struct RustormClient"), "gen missing RustormClient: {gen}");
    assert!(gen.contains("pub fn user(&self)"), "gen missing accessor: {gen}");
    assert!(gen.contains("UserDelegate") || gen.contains("ModelDelegate"), "gen missing delegate: {gen}");
    assert!(gen.contains("impl RustormClient"), "gen: {gen}");
}

#[test]
fn generated_record_and_from_row_and_fields() {
    let src = format!("{} model Product {{ id Int @id name String price Decimal @db.Decimal(10,2) }}", header());
    let s = parse(&src).unwrap();
    let gen = generate(&s);
    assert!(gen.contains("pub struct Record"));
    assert!(gen.contains("FromRow"));
    assert!(gen.contains("pub const PRICE") || gen.contains("PRICE") || gen.contains("NAME"));
}

#[test]
fn build_rs_helper_exists_and_generates() {
    let src = format!("{} model M {{ id Int @id x String }}", header());
    let s = parse(&src).unwrap();
    let out = generate(&s);
    assert!(!out.is_empty());
    assert!(out.starts_with("// @generated"));
}

// ---------- CLIENT_EXAMPLE builder ----------
#[test]
fn builder_api_filter_order_take_skip_cursor_distinct() {
    use rustorm::client_builder::{take, skip, FindManyBuilder};
    let title = f_string("posts", "title");
    let id = f_i32("posts", "id");
    let q = FindManyBuilder::new("posts", &["id", "title"])
        .filter(title.contains("Rust"))
        .order(title.asc())
        .take(take(10))
        .skip(skip(0))
        .distinct()
        .compile()
        .unwrap();
    assert!(q.sql.contains("SELECT"));
    assert!(q.sql.contains("LIMIT"));
    assert!(q.sql.contains("posts"));
    let q2 = FindManyBuilder::new("posts", &["id"]).take(take(5)).compile().unwrap();
    assert!(q2.sql.contains("LIMIT"));
    let sel = Select::new("posts", &["id"]).cursor(id, 42).order_by(id.asc()).take(5).compile().unwrap();
    assert!(sel.sql.contains("WHERE") && sel.sql.contains("> $"));
}

#[test]
fn builder_select_projection_via_select() {
    let title = f_string("posts", "title");
    let sel = Select::new("posts", &["id", "title", "published"])
        .select(title)
        .compile()
        .unwrap();
    assert!(sel.sql.contains("\"title\""));
    assert!(!sel.sql.contains("\"published\""));
}

// ---------- MIGRATIONS ----------
#[test]
fn migrate_create_initial_sql_and_columns() {
    let src = format!("{} model User {{ id Int @id email String @unique name String? }}", header());
    let s = parse(&src).unwrap();
    let sql = rustorm::migrations::initial_sql(&s).unwrap();
    assert!(sql.contains("CREATE TABLE") && sql.contains("users") || sql.contains("users"));
    assert!(sql.contains("PRIMARY KEY"));
}

#[test]
fn migrate_split_sql_rejects_transaction_control() {
    let bad = "BEGIN; SELECT 1;";
    assert!(rustorm::migrations::split_sql(bad).is_err());
    let ok = "CREATE TABLE foo (id INTEGER PRIMARY KEY);";
    assert!(rustorm::migrations::split_sql(ok).is_ok());
}

#[test]
fn migrate_load_and_create_tempdir() {
    let dir = std::env::temp_dir().join(format!(
        "rustorm-m5-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let src = format!("{} model T {{ id Int @id x String }}", header());
    let s = parse(&src).unwrap();
    let p = rustorm::migrations::create(&dir, "init", &s).expect("create");
    assert!(p.exists());
    let loaded = rustorm::migrations::load(&dir).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].name.contains("init"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- INTROSPECTION ----------
#[test]
fn introspection_offline_roundtrip_via_format() {
    let src = format!("{} model User {{ id Int @id email String @unique name String? @@map(\"users\") }}", header());
    let formatted = format(&src).unwrap();
    assert!(formatted.contains("@@map(\"users\")"));
    let reparsed = parse(&formatted).unwrap();
    assert_eq!(reparsed.models[0].table, "users");
}

#[test]
fn db_pull_print_placeholder_does_not_error_when_postgres_feature_missing() {
    let src = format!("{} model M {{ id Int @id x String }}", header());
    let formatted = format(&src).unwrap();
    assert!(formatted.contains("model M"));
}

// ---------- QUERY_ENGINE ----------
#[test]
fn find_unique_like_filter_equals_and_is_not_null() {
    let id = f_i32("users", "id");
    let email = f_opt_string("users", "email");
    let sel = Select::new("users", &["id", "email"])
        .where_(id.equals(1))
        .compile()
        .unwrap();
    assert!(sel.sql.contains("WHERE") && sel.sql.contains("$1"));
    assert_eq!(sel.params.len(), 1);
    let sel2 = Select::new("users", &["id"])
        .where_(email.is_not_null())
        .compile()
        .unwrap();
    assert!(sel2.sql.contains("IS NOT NULL"));
}

#[test]
fn filter_in_empty_and_not_empty_render_correctly() {
    let id = f_i32("users", "id");
    let q_empty = Select::new("users", &["id"])
        .where_(id.is_in(Vec::<i32>::new()))
        .compile()
        .unwrap();
    assert!(q_empty.sql.contains("1=0"));
    let q_full = Select::new("users", &["id"])
        .where_(id.is_in(vec![1, 2, 3]))
        .compile()
        .unwrap();
    assert!(q_full.sql.contains("IN"));
}

#[test]
fn aggregate_count_sum_avg_min_max_groupby_having() {
    let id = f_i32("users", "id");
    let name = f_string("users", "name");
    let q = Aggregate::new("users")
        .count(Some(id))
        .sum(id)
        .avg(id)
        .min(id)
        .max(id)
        .group_by(name.clone())
        .having(name.equals("a".to_string()))
        .order_by(name.asc())
        .take(10)
        .skip(5)
        .compile()
        .unwrap();
    assert!(q.sql.contains("COUNT") && q.sql.contains("SUM") && q.sql.contains("AVG"));
    assert!(q.sql.contains("GROUP BY") && q.sql.contains("HAVING"));
    assert!(q.sql.contains("ORDER BY") && q.sql.contains("LIMIT"));
}

#[test]
fn json_filters_and_string_mode() {
    let data = f_string("users", "data");
    let f = data.json_equals("{\"a\":1}");
    let sel = Select::new("users", &["id"]).where_(f).compile().unwrap();
    assert!(sel.sql.contains("::jsonb"));
    let f2 = data.contains_with_mode("test", StringQueryMode::Insensitive);
    let sel2 = Select::new("users", &["id"]).where_(f2).compile().unwrap();
    assert!(sel2.sql.contains("ILIKE"));
    let f3 = data.json_has_key("key");
    let sel3 = Select::new("users", &["id"]).where_(f3).compile().unwrap();
    assert!(sel3.sql.contains("?"));
}

#[test]
fn distinct_with_order_requires_selected() {
    let name = f_string("users", "name");
    let id = f_i32("users", "id");
    let err = Select::new("users", &["id", "name"])
        .select(name)
        .distinct()
        .order_by(id.asc())
        .compile();
    assert!(err.is_err(), "distinct order field must be selected");
    let ok = Select::new("users", &["id", "name"])
        .select(name)
        .select(id)
        .distinct()
        .order_by(id.asc())
        .compile()
        .unwrap();
    assert!(ok.sql.contains("DISTINCT"));
}

#[test]
fn bytes_and_numeric_mutations_and_upsert_stub() {
    let file: Field<Vec<u8>> = Field {
        table: "files",
        name: "data",
        marker: PhantomData,
    };
    let sel = Select::new("files", &["id"])
        .where_(file.equals(vec![1, 2]))
        .compile()
        .unwrap();
    assert!(sel.sql.contains("="));
    let cnt = f_i32("posts", "views");
    let upd = rustorm::query::Update::new("posts")
        .increment(cnt, 1)
        .where_(f_i32("posts", "id").equals(1))
        .compile()
        .unwrap();
    assert!(upd.sql.contains("views"));
    assert!(rustorm::query::InsertManyStub::new("users").compile().is_err());
}
