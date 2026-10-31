use rustorm::schema::parse;

#[test]
fn composite_fk_parsed_and_validated() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" }
model Author { id Int @id posts Post[] }
model Post { id Int @id authorId Int author Author @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: Cascade) }
"#;
    // Single FK still works (baseline)
    let schema = parse(src).unwrap();
    assert_eq!(schema.models.len(), 2);
}

#[test]
fn composite_fk_multiple_fields() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" }
model A { id Int second String @unique a1 Int a2 String posts Post[] @@id([id, second]) }
model Post { id Int @id a1 Int a2 String a A @relation(fields: [a1, a2], references: [id, second], onDelete: Cascade, onUpdate: Cascade) }
"#;
    let schema = parse(src).expect("composite fk should parse");
    // Check that FK field contains comma
    let rel = &schema.models[1].relations[0];
    assert!(rel.foreign_key.as_ref().unwrap().field.contains(","), "{:?}", rel.foreign_key);
}

#[test]
fn named_m2m_allowed() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" }
model Post { id Int @id tags Tag[] @relation("PostTags") }
model Tag { id Int @id posts Post[] @relation("PostTags") }
"#;
    let schema = parse(src).expect("named m2m allowed");
    assert_eq!(schema.models.len(), 2);
    let join_name = rustorm::schema::implicit_join_name_for_relation(Some("PostTags"), "Post", "Tag");
    assert!(join_name.starts_with("rustorm_m2m_"));
}

#[test]
fn self_m2m_allowed_with_name() {
    let src = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" }
model User { id Int @id friends User[] @relation("Friends") friendOf User[] @relation("Friends") }
"#;
    let schema = parse(src).expect("self m2m with name allowed");
    assert_eq!(schema.models[0].relations.len(), 2);
}

#[test]
fn implicit_join_name_stable() {
    let a = rustorm::schema::implicit_join_name("Post", "Tag");
    let b = rustorm::schema::implicit_join_name("Tag", "Post");
    assert_eq!(a, b, "orientation independent");
}
