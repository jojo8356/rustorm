use rustorm::{migrations, schema};
const H: &str = "datasource db {\n provider = \"postgresql\"\n}\n";
const TWO: &str = r#"
model User {
 id Int @id
 authored Post[] @relation("Written")
 edited Post[] @relation("Edited")
}
model Post {
 id Int @id
 authorId Int
 editorId Int
 author User @relation("Written", fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 editor User @relation("Edited", fields: [editorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
}
"#;
const TREE: &str = r#"
model Node {
 id Int @id
 parentId Int?
 parent Node? @relation("Tree", fields: [parentId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 children Node[] @relation("Tree")
}
"#;
fn parse(s: &str) -> Result<schema::Schema, schema::Diagnostic> {
    schema::parse(&format!("{H}{s}"))
}
#[test]
fn named_relations_connect_correct_inverses() {
    let schema = parse(TWO).unwrap();
    let sql = migrations::initial_sql(&schema).unwrap();
    assert!(sql.contains("FOREIGN KEY (\"authorId\") REFERENCES \"User\" (\"id\")"));
    assert!(sql.contains("FOREIGN KEY (\"editorId\") REFERENCES \"User\" (\"id\")"));
    let generated = schema::generate(&schema);
    assert!(
        generated.contains("Many::new(\"User\", \"id\", \"Post\", \"authorId\""),
        "{generated}"
    );
    assert!(
        generated.contains("Many::new(\"User\", \"id\", \"Post\", \"editorId\""),
        "{generated}"
    );
}
#[test]
fn named_self_relation_is_unambiguous_and_generates_both_sides() {
    let schema = parse(TREE).unwrap();
    assert!(migrations::initial_sql(&schema)
        .unwrap()
        .contains("FOREIGN KEY (\"parentId\") REFERENCES \"Node\" (\"id\")"));
    let client = schema::generate(&schema);
    assert!(
        client.contains(
            "pub fn children() -> ::rustorm::relations::Many<Record, super::node::Record, i32>"
        ),
        "{client}"
    );
    assert!(client.contains("Many::new(\"Node\", \"id\", \"Node\", \"parentId\""));
    assert!(client.contains(
        "pub fn parent() -> ::rustorm::relations::One<Record, super::node::Record, i32>"
    ));
}
#[test]
fn ambiguous_unpaired_or_unsupported_names_are_rejected() {
    for input in [
        TWO.replace("@relation(\"Edited\")", "@relation(\"Written\")"),
        TWO.replace("@relation(\"Edited\")", "@relation(\"Wrong\")"),
        TWO.replace("@relation(\"Edited\")", "@relation(\"\")"),
        TWO.replace("edited Post[] @relation(\"Edited\")", "edited Post[]"),
        TREE.replace("@relation(\"Tree\")", ""),
        TREE.replace(
            "children Node[] @relation(\"Tree\")",
            "children Node[] @relation(\"Tree\")\n other Node[] @relation(\"Tree\")",
        ),
    ] {
        assert!(parse(&input).is_err(), "{input}");
    }
}
#[test]
fn named_self_one_to_one_resolves_the_correct_owner() {
    let input = r#"
model Person {
 id Int @id
 spouseId Int? @unique
 spouse Person? @relation("Marriage", fields: [spouseId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 spouseOf Person? @relation("Marriage")
}
"#;
    let schema = parse(input).unwrap();
    let generated = schema::generate(&schema);
    assert!(generated.contains("pub fn spouse_of() -> ::rustorm::relations::InverseOne<Record, super::person::Record, i32>"),"{generated}");
    assert!(
        generated.contains("InverseOne::new(\"Person\", \"id\", \"Person\", \"spouseId\""),
        "{generated}"
    );
    assert!(migrations::initial_sql(&schema)
        .unwrap()
        .contains("FOREIGN KEY (\"spouseId\") REFERENCES \"Person\" (\"id\")"));
}
#[test]
fn named_one_to_many_can_coexist_with_unnamed_many_to_many() {
    let input = r#"
model A {
 id Int @id
 links B[] @relation("Primary")
 shared B[]
}
model B {
 id Int @id
 ownerId Int
 owner A @relation("Primary", fields: [ownerId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 shared A[]
}
"#;
    let schema = parse(input).unwrap();
    let generated = schema::generate(&schema);
    assert!(
        generated.contains("Many::new(\"A\", \"id\", \"B\", \"ownerId\""),
        "{generated}"
    );
    assert!(
        generated.contains(
            "pub fn shared() -> ::rustorm::relations::Through<Record, super::b::Record, i32, i32>"
        ),
        "{generated}"
    );
    let sql = migrations::initial_sql(&schema).unwrap();
    assert_eq!(sql.matches("CREATE TABLE \"rustorm_m2m_").count(), 1);
}
