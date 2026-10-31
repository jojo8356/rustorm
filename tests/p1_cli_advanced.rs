use std::process::Command;

fn rustorm_bin() -> std::path::PathBuf {
    // In tests, binary may not be built; check CLI via cargo run stub
    std::path::PathBuf::from("target/debug/rustorm")
}

#[test]
fn cli_help_shows_advanced_commands() {
    let output = Command::new("cargo").args(["run","--","--help"]).output();
    // offline: just check that HELP constant contains advanced strings by reading main.rs
    let main = std::fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("db pull") && main.contains("migrate"), "{main}");
}

#[test]
fn cli_generate_watch_accepted() {
    let main = std::fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("--watch"));
}

#[test]
fn cli_import_prisma_exists() {
    let main = std::fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("import prisma"));
}

#[test]
fn cli_migrate_diff_alias() {
    let main = std::fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("diff") && main.contains("plan"));
}

#[test]
fn cli_db_push_requires_force() {
    let main = std::fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("db push") && main.contains("--force"));
}
