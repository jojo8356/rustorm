use std::fs;
use std::path::PathBuf;

#[test]
fn multifile_schema_merged() {
    let dir = PathBuf::from("/tmp/rustorm_multifile_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.rsm"), r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } model User { id Int @id }"#).unwrap();
    fs::write(dir.join("b.rsm"), r#"model Post { id Int @id title String }"#).unwrap();
    // Use read_schema_input via main helper indirectly: test parse merging
    let mut combined = String::new();
    for p in ["a.rsm","b.rsm"] {
        combined.push_str(&fs::read_to_string(dir.join(p)).unwrap());
        combined.push('\n');
    }
    let schema = rustorm::schema::parse(&combined).unwrap();
    assert_eq!(schema.models.len(), 2);
    let _ = fs::remove_dir_all(&dir);
}
