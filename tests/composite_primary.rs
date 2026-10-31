use rustorm::{migrations, schema};
const HEADER: &str = "datasource db {\n provider = \"postgresql\"\n}\n";
pub const EXPLICIT: &str = r#"
model Post {
 id Int @id
 categories PostCategory[]
 @@map("posts")
}
model Category {
 id String @id
 posts PostCategory[]
 @@map("categories")
}
model PostCategory {
 postId Int @map("post_id")
 categoryId String @map("category_id")
 assignedBy String
 post Post @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 category Category @relation(fields: [categoryId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 @@id([postId, categoryId])
 @@map("post_categories")
}
"#;
fn parse(s: &str) -> Result<schema::Schema, schema::Diagnostic> {
    schema::parse(&format!("{HEADER}{s}"))
}
#[test]
fn explicit_many_to_many_uses_a_composite_primary_key_and_two_foreign_keys() {
    let schema = parse(EXPLICIT).unwrap();
    let link = &schema.models[2];
    assert_eq!(link.primary_key, vec!["postId", "categoryId"]);
    let sql = migrations::initial_sql(&schema).unwrap();
    assert!(
        sql.contains("PRIMARY KEY (\"post_id\", \"category_id\")"),
        "{sql}"
    );
    assert!(
        sql.contains("FOREIGN KEY (\"post_id\") REFERENCES \"posts\" (\"id\")"),
        "{sql}"
    );
    assert!(
        sql.contains("FOREIGN KEY (\"category_id\") REFERENCES \"categories\" (\"id\")"),
        "{sql}"
    );
    let client = schema::generate(&schema);
    assert!(client.contains("pub fn categories() -> ::rustorm::relations::Many<Record, super::post_category::Record, i32>"));
    assert!(client
        .contains("pub fn post() -> ::rustorm::relations::One<Record, super::post::Record, i32>"));
    assert!(client.contains(
        "pub fn category() -> ::rustorm::relations::One<Record, super::category::Record, String>"
    ));
}
#[test]
fn invalid_composite_primary_keys_are_rejected() {
    for altered in [
        EXPLICIT.replace("@@id([postId, categoryId])", "@@id([])"),
        EXPLICIT.replace("@@id([postId, categoryId])", "@@id([postId])"),
        EXPLICIT.replace("@@id([postId, categoryId])", "@@id([postId, postId])"),
        EXPLICIT.replace("@@id([postId, categoryId])", "@@id([postId, missing])"),
        EXPLICIT.replace("@@id([postId, categoryId])", "@@id([postId, category])"),
        EXPLICIT.replace(
            "@@id([postId, categoryId])",
            "@@id([postId, categoryId])\n @@id([postId, categoryId])",
        ),
        EXPLICIT.replace("postId Int @map", "postId Int @id @map"),
        EXPLICIT
            .replace("categoryId String @map", "categoryId String? @map")
            .replace(
                "category Category @relation",
                "category Category? @relation",
            ),
    ] {
        assert!(
            parse(&altered).is_err(),
            "accepted invalid schema: {altered}"
        );
    }
}
#[test]
fn forged_composite_primary_keys_fail_closed_before_migration_generation() {
    let good = parse(EXPLICIT).unwrap();
    for keys in [
        vec![],
        vec!["postId"],
        vec!["postId", "postId"],
        vec!["postId", "missing"],
        vec!["postId", "category"],
    ] {
        let mut bad = good.clone();
        bad.models[2].primary_key = keys.into_iter().map(str::to_owned).collect();
        assert!(migrations::initial_sql(&bad).is_err());
    }
    let mut bad = good.clone();
    bad.models[2].fields[1].nullable = true;
    assert!(migrations::initial_sql(&bad).is_err());
}
#[test]
fn composite_primary_columns_are_not_individually_unique() {
    let input = format!("{}\nmodel LinkRef {{\n id Int @id\n linkId Int\n link PostCategory @relation(fields: [linkId], references: [postId], onDelete: Cascade, onUpdate: Cascade)\n}}\n", EXPLICIT.replace("assignedBy String", "assignedBy String\n refs LinkRef[]"));
    assert!(parse(&input).is_err());
}

#[test]
fn postgres_composite_primary_key_column_limit_is_validated() {
    fn model(n: usize) -> String {
        let fields = (0..n).map(|i| format!(" f{i} Int\n")).collect::<String>();
        let names = (0..n).map(|i| format!("f{i}")).collect::<Vec<_>>().join(", ");
        format!("model Bound {{\n{fields} @@id([{names}])\n}}\n")
    }
    assert!(parse(&model(32)).is_ok());
    assert!(parse(&model(33)).is_err());
}
