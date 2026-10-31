use rustorm::{migrations, schema};

const HEADER: &str = "datasource db {\n provider = \"postgresql\"\n}\n";
const RELATIONS: &str = r#"
model Post {
  id Int @id
  title String
  authorId Int @map("author_id")
  author User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
  @@map("posts")
}
model User {
  id Int @id
  email String @unique
  posts Post[]
  @@map("users")
}
"#;
fn parse(s: &str) -> Result<schema::Schema, schema::Diagnostic> {
    schema::parse(&format!("{HEADER}{s}"))
}
#[test]
fn parses_bidirectional_one_to_many_and_generates_scalar_client() {
    let parsed = parse(RELATIONS).unwrap();
    assert_eq!(parsed.models.len(), 2);
    let post = &parsed.models[0];
    assert_eq!(
        post.fields.len(),
        3,
        "relations must not become scalar fields"
    );
    let client = schema::generate(&parsed);
    assert!(client.contains("pub author_id: i32"));
    assert!(!client.contains("pub author: "));
    assert!(!client.contains("pub posts: "));
    assert!(
        client.contains(
            "pub fn posts() -> ::rustorm::relations::Many<Record, super::post::Record, i32>"
        ),
        "{client}"
    );
    assert_eq!(client, schema::generate(&parse(RELATIONS).unwrap()));
}
#[test]
fn creates_foreign_key_after_tables_with_mapped_names_and_actions() {
    let sql = migrations::initial_sql(&parse(RELATIONS).unwrap()).unwrap();
    let first = sql.find("CREATE TABLE \"posts\"").unwrap();
    let second = sql.find("CREATE TABLE \"users\"").unwrap();
    let fk = sql.find("ALTER TABLE \"posts\"").unwrap();
    assert!(fk > first && fk > second);
    assert!(sql.contains("FOREIGN KEY (\"author_id\") REFERENCES \"users\" (\"id\") ON DELETE RESTRICT ON UPDATE CASCADE"), "{sql}");
    assert!(!sql.contains("\"author\" INTEGER"));
}
#[test]
fn validates_relation_targets_inverse_keys_nullability_and_actions() {
    let cases = [
        RELATIONS.replace("posts Post[]", "posts Missing[]"),
        RELATIONS.replace("posts Post[]", "posts Int"),
        RELATIONS.replace("authorId Int @map", "authorId String @map"),
        RELATIONS.replace("references: [id]", "references: [email]"),
        RELATIONS.replace("references: [id]", "references: [missing]"),
        RELATIONS.replace("authorId Int @map", "authorId Int? @map"),
        RELATIONS.replace("onDelete: Restrict", "onDelete: SetNull"),
        RELATIONS.replace("onDelete: Restrict", "onDelete: SetDefault"),
        RELATIONS.replace(", onDelete: Restrict", ""),
        RELATIONS.replace("posts Post[]", "posts Post[]\n  other Post[]"),
        RELATIONS.replace("author User @relation", "author User? @relation"),
        RELATIONS.replace("authorId Int @map", "authorId Int @unique @map"),
        RELATIONS
            .replace("authorId Int @map", "authorId Float @map")
            .replace("id Int @id\n  email", "id Float @id\n  email"),
    ];
    for input in cases {
        assert!(
            parse(&input).is_err(),
            "accepted invalid relation:\n{input}"
        );
    }
}
#[test]
fn nullable_relation_requires_set_null_compatible_foreign_key() {
    let optional = RELATIONS
        .replace("authorId Int @map", "authorId Int? @map")
        .replace("author User @relation", "author User? @relation")
        .replace("onDelete: Restrict", "onDelete: SetNull");
    let sql = migrations::initial_sql(&parse(&optional).unwrap()).unwrap();
    assert!(sql.contains("ON DELETE SET NULL ON UPDATE CASCADE"));
}
#[test]
fn unsupported_relation_forms_fail_closed() {
    let cases = [
        RELATIONS.replace("posts Post[]", "posts Post"),
        RELATIONS.replace("author User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)", "author User"),
        RELATIONS.replace("author User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)", "author User @relation(fields: [authorId, id], references: [id, id], onDelete: Restrict, onUpdate: Cascade)"),
        RELATIONS.replace("posts Post[]", "posts Post[] @relation(\"Named\")"),
    ];
    for input in cases {
        assert!(
            parse(&input).is_err(),
            "accepted unsupported relation:\n{input}"
        );
    }
}

#[test]
fn refuses_relation_names_that_collide_with_generated_methods() {
    for name in [
        "findMany",
        "columns",
        "create",
        "count",
        "updateMany",
        "deleteMany",
    ] {
        let invalid = RELATIONS.replace("posts Post[]", &format!("{name} Post[]"));
        assert!(
            parse(&invalid).is_err(),
            "accepted generated method collision {name}"
        );
    }
}

#[test]
fn forged_relation_and_quoted_identifiers_cannot_generate_unsafe_sql() {
    let mut parsed = parse(RELATIONS).unwrap();
    parsed.models[0].relations[0]
        .foreign_key
        .as_mut()
        .unwrap()
        .field = "missing".into();
    assert!(migrations::initial_sql(&parsed).is_err());
    let mut indexed = parse(&RELATIONS.replace(
        "@@map(\"posts\")",
        "@@index([authorId])\n  @@map(\"posts\")",
    ))
    .unwrap();
    indexed.models[0].indexes[0].field = "nonexistent".into();
    assert!(migrations::initial_sql(&indexed).is_err());
    let quoted = RELATIONS.replace("@@map(\"posts\")", "@@map(\"p\\\"osts\")");
    let sql = migrations::initial_sql(&parse(&quoted).unwrap()).unwrap();
    assert!(sql.contains("ALTER TABLE \"p\"\"osts\""), "{sql}");
}

#[test]
fn relation_foreign_key_index_is_validated_and_generated() {
    let with_index = RELATIONS.replace(
        "@@map(\"posts\")",
        "@@index([authorId])\n  @@map(\"posts\")",
    );
    let parsed = parse(&with_index).unwrap();
    let sql = migrations::initial_sql(&parsed).unwrap();
    assert!(sql.contains("CREATE INDEX \"rustorm_idx_"), "{sql}");
    assert!(sql.contains(" ON \"posts\" (\"author_id\")"), "{sql}");
    for input in [
        with_index.replace("@@index([authorId])", "@@index([missing])"),
        with_index.replace("@@index([authorId])", "@@index([author])"),
        with_index.replace("@@index([authorId])", "@@index([authorId, title])"),
        with_index.replace(
            "@@index([authorId])",
            "@@index([authorId])\n  @@index([authorId])",
        ),
    ] {
        assert!(parse(&input).is_err(), "accepted invalid index: {input}");
    }
}

#[test]
fn relation_filters_compile_to_correlated_parameterized_exists() {
    use rustorm::query::{Field, Select, Value};
    use rustorm::relations::Many;
    let relation =
        || Many::<(), (), i32>::new("users", "id", "posts", "author_id", &["id", "author_id"]);
    let pid = Field::<i32>::new("posts", "id");
    let q = Select::new("users", &["id"])
        .where_(relation().some(pid.gte(7)))
        .where_(relation().none(pid.equals(1)))
        .where_(relation().every(pid.gt(0)))
        .compile()
        .unwrap();
    assert_eq!(q.sql, "SELECT \"id\" FROM \"users\" WHERE EXISTS (SELECT 1 FROM \"posts\" AS \"__rustorm_rel_0\" WHERE \"__rustorm_rel_0\".\"author_id\" = \"users\".\"id\" AND (\"__rustorm_rel_0\".\"id\" >= $1)) AND NOT EXISTS (SELECT 1 FROM \"posts\" AS \"__rustorm_rel_0\" WHERE \"__rustorm_rel_0\".\"author_id\" = \"users\".\"id\" AND (\"__rustorm_rel_0\".\"id\" = $2)) AND NOT EXISTS (SELECT 1 FROM \"posts\" AS \"__rustorm_rel_0\" WHERE \"__rustorm_rel_0\".\"author_id\" = \"users\".\"id\" AND ((\"__rustorm_rel_0\".\"id\" > $3) IS NOT TRUE))");
    assert_eq!(q.params, vec![Value::Int(7), Value::Int(1), Value::Int(0)]);
    let wrong = Field::<i32>::new("other", "id");
    assert!(Select::new("users", &["id"])
        .where_(relation().some(wrong.equals(1)))
        .compile()
        .is_err());
    assert!(Select::new("other", &["id"])
        .where_(relation().some(pid.equals(1)))
        .compile()
        .is_err());
}

#[test]
fn nested_relation_filters_use_distinct_scopes_and_validate_depth() {
    use rustorm::query::{Field, Filter, QueryError, Select};
    use rustorm::relations::Many;
    let posts = || Many::<(), (), i32>::new("users", "id", "posts", "author_id", &["id"]);
    let likes = || Many::<(), (), i32>::new("posts", "id", "likes", "post_id", &["id"]);
    let query = Select::new("users", &["id"])
        .where_(posts().some(likes().none(Field::<i32>::new("likes", "id").equals(9))))
        .compile()
        .unwrap();
    assert!(
        query
            .sql
            .contains("\"__rustorm_rel_1\".\"post_id\" = \"__rustorm_rel_0\".\"id\""),
        "{}",
        query.sql
    );
    assert_eq!(query.params, vec![rustorm::query::Value::Int(9)]);
    assert_eq!(
        Select::new("users", &["id"])
            .where_(posts().some(Filter::or(vec![])))
            .compile()
            .unwrap_err(),
        QueryError::EmptyFilterGroup
    );
    let mut nested = Field::<i32>::new("users", "id").equals(1);
    // Depth bounds apply to ordinary and relation predicates together.
    for _ in 0..66 {
        nested = Filter::negate(nested);
    }
    assert_eq!(
        Select::new("users", &["id"])
            .where_(nested)
            .compile()
            .unwrap_err(),
        QueryError::FilterTooDeep
    );
    let tricky = Many::<(), (), i32>::new("__rustorm_rel_0", "id", "posts", "author_id", &["id"]);
    let q = Select::new("__rustorm_rel_0", &["id"])
        .where_(tricky.some(Field::<i32>::new("posts", "id").equals(1)))
        .compile()
        .unwrap();
    assert!(q.sql.contains("AS \"__rustorm_rel_0_\""), "{}", q.sql);
}

#[test]
fn generates_owning_side_descriptor_for_required_and_optional_relations() {
    let client = schema::generate(&parse(RELATIONS).unwrap());
    assert!(
        client.contains(
            "pub fn author() -> ::rustorm::relations::One<Record, super::user::Record, i32>"
        ),
        "{client}"
    );
    assert!(
        client.contains("\"users\", \"id\", false, &[\"id\", \"email\", ]"),
        "{client}"
    );
    let optional = RELATIONS
        .replace("authorId Int @map", "authorId Int? @map")
        .replace("author User @relation", "author User? @relation")
        .replace("onDelete: Restrict", "onDelete: SetNull");
    let client = schema::generate(&parse(&optional).unwrap());
    assert!(
        client.contains("\"users\", \"id\", true, &[\"id\", \"email\", ]"),
        "{client}"
    );
}

#[test]
fn generated_inverse_descriptors_carry_fk_nullability() {
    let required = schema::generate(&parse(RELATIONS).unwrap());
    assert!(!required.contains(".with_nullable_fk()"), "{required}");
    let optional = RELATIONS
        .replace("authorId Int @map", "authorId Int? @map")
        .replace("author User @relation", "author User? @relation")
        .replace("onDelete: Restrict", "onDelete: SetNull");
    let generated = schema::generate(&parse(&optional).unwrap());
    assert_eq!(
        generated.matches(".with_nullable_fk()").count(),
        1,
        "{generated}"
    );
}

#[test]
fn generated_disconnect_keys_include_only_scalar_unique_fields() {
    let schema = RELATIONS.replace("title String", "title String @unique");
    let generated = schema::generate(&parse(&schema).unwrap());
    assert!(
        generated.contains(".with_unique_child_fields(&[\"id\", \"title\", ])"),
        "{generated}"
    );
}
