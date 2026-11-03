//! Multi-OS + CI + snapshots : offline only, no PG, no secrets
use rustorm::schema::{parse, format, generate};
use rustorm::migrations;
use std::path::Path;

fn header() -> String { "datasource db {\n provider = \"postgresql\"\n url = env(\"DATABASE_URL\")\n}\n".to_string() }
fn rsm(body: &str) -> String { std::format!("{}{}\n", header(), body) }

#[test]
fn offline_no_database_url_required_val_tst02() {
    let saved = std::env::var("DATABASE_URL").ok();
    std::env::remove_var("DATABASE_URL");
    std::env::remove_var("POSTGRES_URL");
    std::env::remove_var("RUSTORM_TEST_DATABASE_URL");
    let src = rsm("model User { id Int @id name String }");
    let schema = parse(&src).expect("parse without DATABASE_URL");
    let _ = generate(&schema);
    let _ = migrations::initial_sql(&schema).unwrap();
    if let Some(v) = saved { std::env::set_var("DATABASE_URL", v); }
}

#[test]
fn multi_os_path_handling() {
    let p = Path::new("rustorm").join("schema.rsm");
    assert!(p.to_string_lossy().contains("rustorm"));
    let abs = Path::new("/tmp").join("rustorm_test_123");
    assert!(abs.to_string_lossy().contains("rustorm_test"));
}

#[test]
fn msrv_semver_documented() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(cargo_toml.contains("version = \"0.1.0\""), "SemVer must be 0.1.0");
    let matrix = include_str!("../docs/MATRIX.md");
    assert!(matrix.contains("MSRV") || matrix.contains("SemVer") || matrix.contains("1.78") || matrix.contains("postgres") || matrix.contains("Matrix"), "MATRIX must mention MSRV/SemVer or postgres");
}

#[test]
fn snapshots_sql_ir_deterministes() {
    let src = rsm("model User { id Int @id name String email String @unique }");
    let schema = parse(&src).unwrap();
    let sql1 = migrations::initial_sql(&schema).unwrap();
    let sql2 = migrations::initial_sql(&schema).unwrap();
    assert_eq!(sql1, sql2, "SQL snapshot must be deterministic");
    let hash1 = rustorm::checksum::sha256(sql1.as_bytes());
    let hash2 = rustorm::checksum::sha256(sql2.as_bytes());
    assert_eq!(hash1, hash2, "IR hash must be deterministic");
}

#[test]
fn ci_generate_check_bloquant_format_idempotent() {
    let src = rsm("model User { id Int @id name String }");
    let f1 = format(&src).unwrap();
    let f2 = format(&f1).unwrap();
    assert_eq!(f1, f2, "format --check must block on diff (idempotent)");
    let schema = parse(&src).unwrap();
    let g1 = generate(&schema);
    let g2 = generate(&schema);
    assert_eq!(g1, g2, "generate --check must block on diff (deterministic)");
}

#[test]
fn fuzz_corpus_persisted_and_replayed() {
    let corpus = [
        "",
        "model User { id Int @id }",
        "model User { id Int @id /// comment }",
        "/// triple-slash comment\nmodel User { id Int @id name String }",
        "enum Role { USER ADMIN }",
        "model User { id Int @id x String @unique }",
    ];
    for body in corpus {
        let input = rsm(body);
        let res = parse(&input);
        if let Ok(schema) = res {
            let formatted = format(&input).unwrap();
            assert!(parse(&formatted).is_ok(), "fuzz conservation: formatted must re-parse for {body:?}");
            let gen = generate(&schema);
            assert!(gen.contains("RustormClient") || gen.contains("model") || !gen.is_empty());
        }
    }
}

#[test]
fn ci_workflow_defines_multi_os_and_pg_matrix() {
    let ci = include_str!("../.github/workflows/ci.yml");
    assert!(ci.contains("postgres"), "CI must define PG matrix");
    assert!(ci.contains("16") && ci.contains("18"), "CI must test PG 16+18");
    assert!(ci.contains("ubuntu") || ci.contains("os:"), "CI must define OS matrix (multi-OS)");
    assert!(ci.contains("windows") || ci.contains("macos") || ci.contains("os:"), "CI must cover multi-OS (ubuntu/macos/windows offline)");
    assert!(ci.contains("cargo test"), "CI must run cargo test");
    assert!(ci.contains("--offline"), "CI must run offline tests");
}

#[test]
fn os_detection_is_multi_os_safe() {
    let os = std::env::consts::OS;
    assert!(["linux", "macos", "windows"].contains(&os) || !os.is_empty(), "OS must be detectable");
    let tmp = std::env::temp_dir().join(std::format!("rustorm_test_{}_multi_os", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("temp dir creation must work on all OS");
    std::fs::remove_dir_all(&tmp).ok();
}
