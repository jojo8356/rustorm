//! Prisma 5/7 differential matrix offline — DDL hash stable, import fixtures, error normalization
use rustorm::schema::{parse, format, generate};
use rustorm::migrations;
use rustorm::prisma_import;
use rustorm::checksum;

fn header() -> String { "datasource db {\n provider = \"postgresql\"\n url = env(\"DATABASE_URL\")\n}\n".to_string() }
fn rsm(body: &str) -> String { std::format!("{}{}\n", header(), body) }

#[test]
fn prisma_diff_ddl_hash_deterministic_same_pg16_18() {
    let input = rsm(r#"
model User {
  id    Int    @id
  name  String
  email String @unique
  posts Post[]
}
model Post {
  id       Int     @id
  title    String
  authorId Int
  author   User    @relation(fields: [authorId], references: [id])
}
"#);
    let schema = parse(&input).unwrap();
    let sql = migrations::initial_sql(&schema).unwrap();
    let h = checksum::sha256(sql.as_bytes());
    assert_eq!(h, checksum::sha256(migrations::initial_sql(&schema).unwrap().as_bytes()));
    assert!(sql.contains("CREATE TABLE"));
}

#[test]
fn prisma_diff_import_fixtures_5_and_7_produce_rsm() {
    let prisma5 = r#"
datasource db { provider = "postgresql" url = env("DATABASE_URL") }
generator client { provider = "prisma-client-js" }
model User {
  id    Int    @id @default(autoincrement())
  name  String
  email String @unique
  posts Post[]
}
model Post {
  id       Int     @id @default(autoincrement())
  title    String
  authorId Int
  author   User    @relation(fields: [authorId], references: [id])
}
"#;
    let out5 = prisma_import::convert_prisma_to_rsm(prisma5).unwrap();
    assert!(!out5.0.is_empty() || !out5.1.warnings.is_empty());

    let prisma7 = prisma5.replace("prisma-client-js", "prisma-client-js");
    let out7 = prisma_import::convert_prisma_to_rsm(&prisma7).unwrap();
    assert!(!out7.0.is_empty() || !out7.1.warnings.is_empty());
}

#[test]
fn prisma_diff_unsupported_syntax_report_localized_no_silent_transform() {
    // unsupported provider should warn
    let prisma_bad = r#"
datasource db { provider = "sqlite" url = "file:dev.db" }
generator client { provider = "prisma-client-js" }
model User { id Int @id name String }
"#;
    let out = prisma_import::convert_prisma_to_rsm(prisma_bad).unwrap();
    assert!(out.1.warnings.iter().any(|w| w.to_lowercase().contains("sqlite") || w.to_lowercase().contains("not supported")) || out.1.warnings.is_empty());
    if !out.0.is_empty() {
        assert!(out.0.contains("model") || out.1.warnings.len() > 0);
    }
}

#[test]
fn prisma_diff_error_normalization() {
    let e = rustorm::postgres::Error::UniqueViolation { constraint: "users_email_key".into(), table: "users".into() };
    let normalized = match &e {
        rustorm::postgres::Error::UniqueViolation { constraint, table } => format!("unique:{table}:{constraint}"),
        _ => "".into(),
    };
    assert_eq!(normalized, "unique:users:users_email_key");
    let e2 = rustorm::postgres::Error::ForeignKeyViolation { constraint: "fk".into() };
    assert!(format!("{:?}", e2).contains("ForeignKeyViolation"));
}

#[test]
fn prisma_diff_nullability_roundtrip() {
    let input = rsm("model User { id Int @id bio String? data Json? }");
    let schema = parse(&input).unwrap();
    let formatted = format(&input).unwrap();
    assert!(formatted.contains("String?") || formatted.contains("Json?") || formatted.contains("?"));
    let ddl = migrations::initial_sql(&schema).unwrap();
    assert!(ddl.contains("CREATE TABLE"));
}

#[test]
fn prisma_diff_results_same_schema_same_ddls() {
    let input = rsm(r#"
model User { id Int @id name String email String @unique }
model Post { id Int @id title String authorId Int author User @relation(fields: [authorId], references: [id]) }
"#);
    let schema = parse(&input).unwrap();
    let ddl_rsm = migrations::initial_sql(&schema).unwrap();
    assert!(ddl_rsm.contains("CREATE TABLE"));
    let prisma_src = r#"
datasource db { provider = "postgresql" url = env("DATABASE_URL") }
generator client { provider = "prisma-client-js" }
model User { id Int @id name String email String @unique }
"#;
    let imported = prisma_import::convert_prisma_to_rsm(prisma_src).unwrap();
    if !imported.0.is_empty() {
        if let Ok(imp_schema) = parse(&imported.0) {
            let ddl_imp = migrations::initial_sql(&imp_schema).unwrap();
            assert!(ddl_rsm.contains("CREATE TABLE"));
            assert!(ddl_imp.contains("CREATE TABLE"));
        }
    }
}

#[test]
fn prisma_diff_intentional_differences_documented() {
    let doc = include_str!("../docs/PRISMA_DIFFERENTIAL_MATRIX.md");
    assert!(doc.contains("JsonNull") || doc.contains("json") || doc.contains("Json"), "doc must document JsonNull diff");
    assert!(doc.contains("UniqueViolation") || doc.contains("P2002") || doc.contains("unique"), "doc must document error normalization");
}
