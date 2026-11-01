use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rustorm-cli-complete-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rustorm"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn path(&self, p: &str) -> PathBuf { self.0.join(p) }
}
impl Drop for Temp {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}
fn success(out: &Output) {
    assert!(
        out.status.success(),
        "expected success, got code {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
fn failure(out: &Output) {
    assert!(
        !out.status.success(),
        "expected failure but succeeded\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
fn code(out: &Output) -> Option<i32> { out.status.code() }

// ---------- format ----------
#[test]
fn format_is_idempotent_and_check_returns_2() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // intentionally unformatted: add extra spaces
    let schema_path = t.path("rustorm/schema.rsm");
    let raw = fs::read_to_string(&schema_path).unwrap();
    let unformatted = raw.replace("  provider = \"postgresql\"", "provider = \"postgresql\"");
    fs::write(&schema_path, &unformatted).unwrap();
    // format should fix it
    let out = t.run(&["format"]);
    success(&out);
    let formatted = fs::read_to_string(&schema_path).unwrap();
    assert!(formatted.contains("provider = \"postgresql\""));
    // second format is idempotent
    let out2 = t.run(&["format"]);
    success(&out2);
    assert!(String::from_utf8_lossy(&out2.stdout).contains("Already formatted"));
    // check on formatted should succeed
    success(&t.run(&["format", "--check"]));
    // unformat again and check should fail with code 2
    fs::write(&schema_path, &unformatted).unwrap();
    let out3 = t.run(&["format", "--check"]);
    assert_eq!(out3.status.code(), Some(2), "stderr: {} stdout: {}", String::from_utf8_lossy(&out3.stderr), String::from_utf8_lossy(&out3.stdout));
    // after format, check passes again
    success(&t.run(&["format"]));
    success(&t.run(&["format", "--check"]));
    // format with explicit --schema
    success(&t.run(&["format", "--schema", "rustorm/schema.rsm"]));
    success(&t.run(&["format", "--schema", "rustorm/schema.rsm", "--check"]));
}

#[test]
fn format_via_schema_directives_is_idempotent() {
    // pure unit test without CLI
    let input = "datasource db {\n provider = \"postgresql\"\n url = env(\"DATABASE_URL\")\n}\nmodel User {\n id Int @id @default(autoincrement())\n email String @unique\n}\n";
    let f1 = rustorm::schema::format(input).unwrap();
    let f2 = rustorm::schema::format(&f1).unwrap();
    assert_eq!(f1, f2);
}

// ---------- rustorm.toml ----------
#[test]
fn rustorm_toml_config_priority_cli_over_config() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // create custom schema path and config pointing to it
    let custom_dir = t.path("custom");
    fs::create_dir_all(&custom_dir).unwrap();
    let custom_schema = custom_dir.join("my.rsm");
    fs::copy(t.path("rustorm/schema.rsm"), &custom_schema).unwrap();
    // write rustorm.toml pointing to custom
    fs::write(t.path("rustorm.toml"), "schema = \"custom/my.rsm\"\nmigrationsDir = \"custom/migs\"\n").unwrap();
    // validate without --schema should use custom (priority config > default)
    let out = t.run(&["validate"]);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Valid"));
    // format without --schema should use custom
    success(&t.run(&["format"]));
    // CLI --schema overrides config: point to original, should still validate
    success(&t.run(&["validate", "--schema", "rustorm/schema.rsm"]));
    // explicitly check that config file is read: modify custom to invalid and validate should fail
    fs::write(&custom_schema, "invalid content").unwrap();
    failure(&t.run(&["validate"]));
    // but explicit --schema to valid file still succeeds
    success(&t.run(&["validate", "--schema", "rustorm/schema.rsm"]));
    // test migrationsDir config: migrate create should use custom dir
    fs::write(&custom_schema, fs::read_to_string(t.path("rustorm/schema.rsm")).unwrap()).unwrap();
    let out = t.run(&["migrate", "create", "--name", "init"]);
    success(&out);
    // should have created in custom/migs per config
    assert!(t.path("custom/migs").exists(), "expected custom migrations dir from rustorm.toml");
    // CLI --dir overrides config
    let out = t.run(&["migrate", "create", "--name", "second", "--dir", "rustorm/migrations"]);
    success(&out);
    assert!(t.path("rustorm/migrations").exists());
}

#[test]
fn rustorm_toml_supports_both_naming_conventions() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // use snake_case variant
    fs::write(t.path("rustorm.toml"), "schema = \"rustorm/schema.rsm\"\nmigrations_dir = \"my_migrations\"\n").unwrap();
    let out = t.run(&["migrate", "create", "--name", "init"]);
    success(&out);
    assert!(t.path("my_migrations").exists());
}

// ---------- migrate dev (native) ----------
#[test]
fn migrate_dev_is_native_and_creates_migrations() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // dev should work like create, offline, and create initial SQL
    let out = t.run(&["migrate", "dev", "--name", "initial"]);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Created"));
    let mig_dir = t.path("rustorm/migrations");
    let entries: Vec<_> = fs::read_dir(&mig_dir).unwrap().collect();
    assert_eq!(entries.len(), 1);
    let first_path = entries[0].as_ref().unwrap().path().join("migration.sql");
    let sql = fs::read_to_string(&first_path).unwrap();
    assert!(sql.contains("CREATE TABLE"));
    // dev without --name should fail
    failure(&t.run(&["migrate", "dev"]));
    failure(&t.run(&["migrate", "dev", "--name", "../evil"]));
    // second dev should create empty template requiring review
    success(&t.run(&["migrate", "dev", "--name", "second"]));
    let entries: Vec<_> = fs::read_dir(&mig_dir).unwrap().collect();
    assert_eq!(entries.len(), 2);
    // check that dev is documented in help
    let help = t.run(&["--help"]);
    let h = String::from_utf8_lossy(&help.stdout);
    assert!(h.contains("migrate dev"), "help should list migrate dev natively");
}

#[test]
fn migrate_create_still_works_and_dev_alias_consistent() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    success(&t.run(&["migrate", "dev", "--name", "second"]));
    let dir = t.path("rustorm/migrations");
    let count = fs::read_dir(&dir).unwrap().count();
    assert_eq!(count, 2);
}

// ---------- migrate resolve / baseline checksum ----------
#[test]
fn migrate_resolve_and_baseline_require_confirm_and_fail_offline_without_db() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    // baseline without --confirm should fail (offline, no DB)
    let out = t.run(&["migrate", "baseline", "--name", "x"]);
    failure(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("baseline requires"));
    let out = t.run(&["migrate", "baseline", "--confirm"]);
    failure(&out);
    // resolve without required flags
    failure(&t.run(&["migrate", "resolve", "--name", "x", "--applied"]));
    failure(&t.run(&["migrate", "resolve", "--name", "x", "--applied", "--rolled-back", "--confirm"]));
    failure(&t.run(&["migrate", "resolve", "--name", "x", "--applied", "--confirm"]));
    // with DB missing, should fail with postgres required (if not postgres feature) or DATABASE_URL required
    let out = t.run(&["migrate", "baseline", "--name", "initial", "--confirm"]);
    failure(&out);
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("postgres") || msg.contains("DATABASE_URL"), "got: {msg}");
    let out = t.run(&["migrate", "resolve", "--name", "initial", "--applied", "--confirm"]);
    failure(&out);
}

#[test]
fn checksum_is_stable_and_load_detects_files() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    let dir = t.path("rustorm/migrations");
    let files = rustorm::migrations::load(&dir).unwrap();
    assert_eq!(files.len(), 1);
    assert!(!files[0].checksum.is_empty());
    let expected = rustorm::checksum::sha256(files[0].sql.as_bytes());
    assert_eq!(files[0].checksum, expected);
    // create second migration
    success(&t.run(&["migrate", "create", "--name", "second"]));
    let files2 = rustorm::migrations::load(&dir).unwrap();
    assert_eq!(files2.len(), 2);
    assert!(files2[0].name < files2[1].name);
    // load should reject stray files
    fs::write(dir.join("stray.sql"), "SELECT 1").unwrap();
    let err = rustorm::migrations::load(&dir).unwrap_err();
    assert!(err.to_string().contains("unexpected migration entry"));
}

// ---------- migrate deploy --preview ----------
#[test]
fn migrate_deploy_preview_offline_prints_sql_without_applying() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    success(&t.run(&["migrate", "create", "--name", "second"]));
    let dir = t.path("rustorm/migrations");
    let files_before: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    // preview should succeed offline and print SQL
    let out = t.run(&["migrate", "deploy", "--preview"]);
    success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Preview") || stdout.contains("Pending") || stdout.contains("CREATE TABLE") || stdout.contains("Add reviewed SQL"));
    // ensure no files were modified or removed after preview
    let files_after: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(files_before, files_after);
    // preview should also work with custom --dir
    let out = t.run(&["migrate", "deploy", "--dir", "rustorm/migrations", "--preview"]);
    success(&out);
    // preview with no migrations
    let t2 = Temp::new();
    success(&t2.run(&["init"]));
    let out = t2.run(&["migrate", "deploy", "--preview"]);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("No pending"));
    // deploy without preview and without DB should fail (requires postgres)
    let out = t.run(&["migrate", "deploy"]);
    failure(&out);
}

#[test]
fn migrate_status_and_deploy_require_db() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    let out = t.run(&["migrate", "status"]);
    failure(&out);
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("postgres") || msg.contains("DATABASE_URL"));
}

// ---------- db pull ----------
#[test]
fn db_pull_print_shows_formatted_schema() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // db pull --print should work offline and show formatted schema
    let out = t.run(&["db", "pull", "--print"]);
    success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("model User") && stdout.contains("datasource db"));
    // with explicit --schema
    let out = t.run(&["db", "pull", "--schema", "rustorm/schema.rsm", "--print"]);
    success(&out);
    // without --print and without DB should fail with helpful message
    let out = t.run(&["db", "pull"]);
    failure(&out);
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("DATABASE_URL") || msg.contains("--print") || msg.contains("postgres"));
    // --force without --print still requires DB (offline should fail)
    let out = t.run(&["db", "pull", "--force"]);
    failure(&out);
}

#[test]
fn db_pull_respects_rustorm_toml() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    fs::write(t.path("rustorm.toml"), "schema = \"custom.rsm\"\n").unwrap();
    // create custom schema
    fs::copy(t.path("rustorm/schema.rsm"), t.path("custom.rsm")).unwrap();
    let out = t.run(&["db", "pull", "--print"]);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("model User"));
    // explicit CLI overrides
    let out = t.run(&["db", "pull", "--schema", "rustorm/schema.rsm", "--print"]);
    success(&out);
}

// ---------- P1: migrate diff, db push, migrate reset, import prisma ----------
#[test]
fn p1_migrate_diff_alias_and_db_push_reset() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    // diff should be alias for plan --print (offline fallback: without DB, plan requires DB, so should fail but not panic)
    let out = t.run(&["migrate", "diff", "--print"]);
    // diff without DB will fail with postgres/DATABASE_URL required, but should not be unknown action
    failure(&out);
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(!msg.contains("unknown migrate action"), "diff should be known: {msg}");
    // db push without --force should fail
    let out = t.run(&["db", "push"]);
    failure(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
    // db push with --force without DB should fail with DB message, not unknown
    let out = t.run(&["db", "push", "--force"]);
    failure(&out);
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("postgres") || msg.contains("DATABASE_URL"));
    // migrate reset without --force should fail
    let out = t.run(&["migrate", "reset"]);
    failure(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
    // with --force without DB should fail with DB message
    let out = t.run(&["migrate", "reset", "--force"]);
    failure(&out);
    let msg = String::from_utf8_lossy(&out.stderr);
    assert!(msg.contains("postgres") || msg.contains("DATABASE_URL"));
}

#[test]
fn import_prisma_converts_and_reports() {
    let t = Temp::new();
    let prisma_content = r#"datasource db {
  provider = "postgresql"
  url = env("DATABASE_URL")
}
generator client {
  provider = "prisma-client-js"
}
model User {
  id Int @id @default(autoincrement())
  email String @unique
}
"#;
    let prisma_path = t.path("schema.prisma");
    fs::write(&prisma_path, prisma_content).unwrap();
    let out = t.run(&["import", "prisma", "--from", "schema.prisma", "--to", "imported.rsm"]);
    success(&out);
    let imported = fs::read_to_string(t.path("imported.rsm")).unwrap();
    assert!(imported.contains("rustorm-client"));
    assert!(imported.contains("model User"));
    // --force overwrite
    let out = t.run(&["import", "prisma", "--from", "schema.prisma", "--to", "imported.rsm"]);
    failure(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("exists"));
    success(&t.run(&["import", "prisma", "--from", "schema.prisma", "--to", "imported.rsm", "--force"]));
    // test prisma import logic directly
    let (rsm, report) = rustorm::prisma_import::convert_prisma_to_rsm(prisma_content).unwrap();
    assert!(rsm.contains("rustorm-client"));
    assert_eq!(report.warnings.len(), 0);
}

#[test]
fn p1_db_import_sql_and_migrate_squash() {
    let t = Temp::new();
    // db import --sql
    let sql = "CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT);";
    fs::write(t.path("input.sql"), sql).unwrap();
    let out = t.run(&["db", "import", "--sql", "input.sql", "--to", "from_sql.rsm"]);
    success(&out);
    let rsm = fs::read_to_string(t.path("from_sql.rsm")).unwrap();
    assert!(rsm.contains("model User") || rsm.contains("model user") || rsm.contains("datasource"));
    // squash
    let dir = t.path("rustorm/migrations");
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "init"]));
    success(&t.run(&["migrate", "create", "--name", "add"]));
    let entries: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(entries.len(), 2);
    // find first migration name for --from
    let first_name = entries[0].file_name().unwrap().to_string_lossy().to_string();
    let out = t.run(&["migrate", "squash", "--name", "squashed", "--from", &first_name]);
    success(&out);
    let after: Vec<_> = fs::read_dir(&dir).unwrap().collect();
    assert_eq!(after.len(), 3);
    // strategy flag should be accepted (P2 stub)
    let out = t.run(&["migrate", "deploy", "--strategy", "rolling", "--preview"]);
    success(&out); // preview succeeds even with strategy warning
}

// ---------- P2: generate --watch, --shadow-url, --strategy, studio ----------
#[test]
fn p2_generate_watch_and_shadow_url_and_studio() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // generate --watch should succeed with warning
    let out = t.run(&["generate", "--watch"]);
    success(&out);
    assert!(fs::read_to_string(t.path("src/generated/rustorm/mod.rs")).unwrap().contains("pub mod user"));
    // generate --shadow-url should succeed with warning
    let out = t.run(&["generate", "--shadow-url", "postgres://shadow"]);
    success(&out);
    // migrate plan with --shadow-url (P2) should be accepted (even if fails due to DB)
    success(&t.run(&["migrate", "create", "--name", "init"]));
    let out = t.run(&["migrate", "plan", "--name", "add", "--shadow-url", "postgres://shadow"]);
    // without DB it will fail with DB required, but not unknown option
    failure(&out);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("unknown migration option"));
    // studio
    let out = t.run(&["studio", "--port", "5555"]);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("5542") || String::from_utf8_lossy(&out.stdout).contains("5555"));
    let out = t.run(&["studio", "--port", "5543", "--schema", "rustorm/schema.rsm"]);
    success(&out);
}

// ---------- validate / generate with connector ----------
#[test]
fn validate_and_generate_handle_connector_and_json() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["validate"]));
    success(&t.run(&["validate", "--json"]));
    let out = t.run(&["validate", "--json"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"code\":0"));
    success(&t.run(&["validate", "--connector", "postgresql"]));
    success(&t.run(&["generate"]));
    success(&t.run(&["generate", "--check"]));
    // generate --check should fail after tampering
    fs::write(t.path("src/generated/rustorm/mod.rs"), "// tampered\n").unwrap();
    let out = t.run(&["generate", "--check"]);
    assert_eq!(out.status.code(), Some(2));
}

// ---------- help and version ----------
#[test]
fn help_shows_all_commands_and_version() {
    let t = Temp::new();
    let out = t.run(&["--help"]);
    success(&out);
    let h = String::from_utf8_lossy(&out.stdout);
    assert!(h.contains("migrate dev"));
    assert!(h.contains("migrate create"));
    assert!(h.contains("migrate deploy"));
    assert!(h.contains("deploy --preview") || h.contains("--preview"));
    assert!(h.contains("migrate resolve"));
    assert!(h.contains("migrate baseline"));
    assert!(h.contains("db pull"));
    assert!(h.contains("db push"));
    assert!(h.contains("format"));
    assert!(h.contains("rustorm.toml"));
    let out = t.run(&["version"]);
    success(&out);
    assert!(String::from_utf8_lossy(&out.stdout).contains("rustorm"));
}

// ---------- migration_lock.toml ----------
#[test]
fn migration_lock_toml_created() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    success(&t.run(&["migrate", "create", "--name", "initial"]));
    // lock file should be sibling to migrations, not inside
    assert!(t.path("rustorm/migration_lock.toml").exists());
    // second create should not overwrite lock
    success(&t.run(&["migrate", "create", "--name", "second"]));
    assert!(t.path("rustorm/migration_lock.toml").exists());
}

// ---------- multi-file schema (P2) ----------
#[test]
fn multifile_schema_via_directory() {
    let t = Temp::new();
    success(&t.run(&["init"]));
    // create directory with two .rsm files
    let schema_dir = t.path("schemas");
    fs::create_dir_all(&schema_dir).unwrap();
    fs::write(schema_dir.join("a.rsm"), "datasource db {\n provider = \"postgresql\"\n url = env(\"DATABASE_URL\")\n}\nmodel User {\n id Int @id @default(autoincrement())\n email String @unique\n posts Post[]\n}\n").unwrap();
    fs::write(schema_dir.join("b.rsm"), "model Post {\n id Int @id @default(autoincrement())\n title String\n authorId Int\n author User @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: Cascade)\n}\n").unwrap();
    // validate via directory should succeed
    let out = t.run(&["validate", "--schema", "schemas"]);
    success(&out);
    // generate via directory with explicit output inside project
    let out = t.run(&["generate", "--schema", "schemas", "--output", "src/generated/rustorm/mod.rs"]);
    success(&out);
}
