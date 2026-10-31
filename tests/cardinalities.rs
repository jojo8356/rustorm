use rustorm::{migrations, schema};

const HEADER: &str = "datasource db {\n provider = \"postgresql\"\n}\n";
const ONE: &str = r#"
model User {
 id Int @id
 profile Profile?
 @@map("users")
}
model Profile {
 id Int @id
 userId Int @unique @map("user_id")
 user User @relation(fields: [userId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 @@map("profiles")
}
"#;
const MANY: &str = r#"
model Post {
 id Int @id
 categories Category[]
 @@map("posts")
}
model Category {
 id String @id
 posts Post[]
 @@map("categories")
}
"#;
fn parse(s: &str) -> Result<schema::Schema, schema::Diagnostic> {
    schema::parse(&format!("{HEADER}{s}"))
}
#[test]
fn one_to_one_requires_unique_foreign_key_and_optional_inverse() {
    let schema = parse(ONE).unwrap();
    let sql = migrations::initial_sql(&schema).unwrap();
    assert!(sql.contains("\"user_id\" INTEGER NOT NULL UNIQUE"), "{sql}");
    assert!(sql.contains("FOREIGN KEY (\"user_id\") REFERENCES \"users\" (\"id\")"));
    let generated = schema::generate(&schema);
    assert!(generated.contains("pub fn profile() -> ::rustorm::relations::InverseOne<Record, super::profile::Record, i32>"), "{generated}");
    assert!(generated
        .contains("pub fn user() -> ::rustorm::relations::One<Record, super::user::Record, i32>"));
    assert!(parse(&ONE.replace("userId Int @unique", "userId Int")).is_err());
    assert!(parse(&ONE.replace("profile Profile?", "profile Profile")).is_err());
    assert!(parse(&ONE.replace("profile Profile?", "profile Profile[]")).is_err());
}
#[test]
fn implicit_many_to_many_has_deterministic_join_and_descriptors() {
    let schema = parse(MANY).unwrap();
    let sql = migrations::initial_sql(&schema).unwrap();
    let name = rustorm::schema::implicit_join_name("Post", "Category");
    assert!(sql.contains(&format!("CREATE TABLE \"{name}\"")), "{sql}");
    assert!(sql.contains("\"a\" TEXT NOT NULL"), "{sql}");
    assert!(sql.contains("\"b\" INTEGER NOT NULL"), "{sql}");
    assert!(sql.contains("PRIMARY KEY (\"a\", \"b\")"), "{sql}");
    assert!(sql.contains("REFERENCES \"categories\" (\"id\") ON DELETE CASCADE ON UPDATE CASCADE"));
    assert!(sql.contains("REFERENCES \"posts\" (\"id\") ON DELETE CASCADE ON UPDATE CASCADE"));
    let client = schema::generate(&schema);
    assert!(client.contains("pub fn categories() -> ::rustorm::relations::Through<Record, super::category::Record, i32, String>"), "{client}");
    assert!(client.contains("pub fn posts() -> ::rustorm::relations::Through<Record, super::post::Record, String, i32>"), "{client}");
    let reversed = MANY[MANY.find("model Category").unwrap()..].to_owned()
        + &MANY[..MANY.find("model Category").unwrap()];
    let reverse_sql = migrations::initial_sql(&parse(&reversed).unwrap()).unwrap();
    assert!(reverse_sql.contains(&format!("CREATE TABLE \"{name}\"")));
    assert_eq!(
        rustorm::schema::implicit_join_name("Post", "Category"),
        rustorm::schema::implicit_join_name("Category", "Post")
    );
}
#[test]
fn reject_ambiguous_or_unsupported_many_to_many() {
    assert!(parse(&MANY.replace(
        "categories Category[]",
        "categories Category[]\n other Category[]"
    ))
    .is_err());
    assert!(parse(&MANY.replace("id String @id", "id Float @id")).is_err());
    assert!(parse(&MANY.replace("posts Post[]", "posts Post?")).is_err());
    assert!(parse("model Node {\n id Int @id\n nodes Node[]\n}\n").is_err());
}

#[test]
fn implicit_join_names_cannot_collide_with_user_tables() {
    let name = schema::implicit_join_name("Post", "Category");
    for suffix in ["", "_pk", "_b_idx"] {
        let input = format!("{MANY}\nmodel Evil {{\n id Int @id\n @@map(\"{name}{suffix}\")\n}}\n");
        assert!(migrations::initial_sql(&parse(&input).unwrap()).is_err());
    }
}
#[test]
fn forged_implicit_relations_and_wrong_owner_are_rejected() {
    let mut schema = parse(MANY).unwrap();
    schema.models[0].fields[0].ty = schema::Scalar::Float;
    assert!(migrations::initial_sql(&schema).is_err());
    let optional = ONE
        .replace("userId Int @unique", "userId Int? @unique")
        .replace("user User @relation", "user User? @relation")
        .replace("onDelete: Cascade", "onDelete: SetNull");
    assert!(parse(&optional).is_ok());
    assert!(parse(&ONE.replace("profile Profile?", "profile Profile? @relation(fields: [id], references: [id], onDelete: Cascade, onUpdate: Cascade)")).is_err());
}
