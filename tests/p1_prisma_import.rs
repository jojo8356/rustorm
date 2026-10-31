use rustorm::prisma_import::convert_prisma_to_rsm;

#[test]
fn prisma_client_js_translated_to_rustorm_client() {
    let prisma = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "prisma-client-js" } model User { id Int @id @default(autoincrement()) email String @unique }"#;
    let (rsm, report) = convert_prisma_to_rsm(prisma).unwrap();
    assert!(rsm.contains("rustorm-client"), "{rsm}");
    assert!(report.warnings.is_empty());
}

#[test]
fn prisma_with_preview_features_warns() {
    let prisma = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "prisma-client-js" previewFeatures = ["metrics"] } model User { id Int @id email String }"#;
    let (_, report) = convert_prisma_to_rsm(prisma).unwrap();
    assert!(!report.warnings.is_empty());
}

#[test]
fn prisma_decimal_mapped() {
    let prisma = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "prisma-client-js" } model Product { id Int @id price Decimal @db.Decimal(10,2) }"#;
    let (rsm, _) = convert_prisma_to_rsm(prisma).unwrap();
    assert!(rsm.contains("Decimal"));
}

#[test]
fn prisma_fixture_prisma7() {
    // Prisma 7 uses same datasource format
    let prisma = r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "prisma-client-js" } model Post { id Int @id title String content String? }"#;
    let (rsm, _) = convert_prisma_to_rsm(prisma).unwrap();
    let schema = rustorm::schema::parse(&rsm).unwrap();
    assert_eq!(schema.models[0].name, "Post");
}
