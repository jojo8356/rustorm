#![cfg(feature = "postgres")]
use rustorm::{
    migrations,
    postgres::Connection,
    query::{CompiledQuery, Value},
    schema,
};
use std::{fs, path::PathBuf, process::Command};
struct Databases {
    admin: Connection,
    target: String,
    shadow: String,
    target_created: bool,
    shadow_created: bool,
    extra: Option<String>,
}
impl Drop for Databases {
    fn drop(&mut self) {
        if let Some(name) = &self.extra {
            let _ = self
                .admin
                .execute_raw(&format!("DROP DATABASE \"{}\"", name));
        }
        if self.shadow_created {
            let _ = self
                .admin
                .execute_raw(&format!("DROP DATABASE \"{}\"", self.shadow));
        }
        if self.target_created {
            let _ = self
                .admin
                .execute_raw(&format!("DROP DATABASE \"{}\"", self.target));
        }
    }
}

static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "rustorm-shadow-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn connect(db: &str) -> Connection {
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT")
        .expect("RUSTORM_TEST_DATABASE_PORT")
        .parse()
        .unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    Connection::connect_local_unencrypted(port, &user, db).unwrap()
}
fn exists(conn: &mut Connection, name: &str) -> bool {
    conn.query(&CompiledQuery {
        sql: "SELECT to_regclass($1) IS NOT NULL AS present".into(),
        params: vec![Value::String(name.into())],
    })
    .unwrap()[0]
        .get("present")
        .unwrap()
}
const BASE: &str = r#"datasource db {
  provider = "postgresql"
}
model User {
  id Int @id @default(autoincrement())
  email String @unique
  published Boolean @default(false)
  @@map("users")
}
"#;
fn schema(s: &str) -> schema::Schema {
    schema::parse(s).unwrap()
}

#[test]
#[ignore = "requires disposable PostgreSQL with CREATEDB privilege"]
fn shadow_planning_additions_drift_conflicts_and_cleanup() {
    let temp = Temp::new();
    let dir = temp.0.join("migrations");
    let admin_db =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let target_name = format!("rustorm_target_{}", std::process::id());
    let shadow_name = format!("rustorm_shadow_{}", std::process::id());
    let mut databases = Databases {
        admin: connect(&admin_db),
        target: target_name.clone(),
        shadow: shadow_name.clone(),
        target_created: false,
        shadow_created: false,
        extra: None,
    };
    databases
        .admin
        .execute_raw(&format!("CREATE DATABASE \"{target_name}\""))
        .unwrap();
    databases.target_created = true;
    databases
        .admin
        .execute_raw(&format!("CREATE DATABASE \"{shadow_name}\""))
        .unwrap();
    databases.shadow_created = true;
    let mut target = connect(&target_name);
    let mut shadow = connect(&shadow_name);
    let wrong_encoding = format!("rustorm_latin1_{}", std::process::id());
    databases.admin.execute_raw(&format!("CREATE DATABASE \"{wrong_encoding}\" ENCODING 'LATIN1' LC_COLLATE 'C' LC_CTYPE 'C' TEMPLATE template0")).unwrap();
    databases.extra = Some(wrong_encoding.clone());
    let mut incompatible = connect(&wrong_encoding);
    let error = migrations::plan(&mut target, &mut incompatible, &dir, &schema(BASE))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("matching database encoding and locale"),
        "{error}"
    );
    drop(incompatible);
    if let Ok(port) = std::env::var("RUSTORM_TEST_SECONDARY_PORT") {
        let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
        let mut different_version =
            Connection::connect_local_unencrypted(port.parse().unwrap(), &user, "postgres")
                .unwrap();
        let error = migrations::plan(&mut target, &mut different_version, &dir, &schema(BASE))
            .unwrap_err()
            .to_string();
        assert!(error.contains("same PostgreSQL major version"), "{error}");
    }
    let mut same = connect(&target_name);
    assert!(migrations::plan(&mut target, &mut same, &dir, &schema(BASE)).is_err());
    drop(same);
    shadow
        .execute_raw("CREATE TABLE occupied (id int)")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(BASE)).is_err());
    assert!(exists(&mut shadow, "public.occupied"));
    shadow.execute_raw("DROP TABLE occupied").unwrap();
    // Starting from no migrations on an empty database yields initial table SQL.
    let first = migrations::plan(&mut target, &mut shadow, &dir, &schema(BASE)).unwrap();
    assert!(first.sql().contains("CREATE TABLE \"users\""));
    assert!(!exists(&mut shadow, "public.users"));
    migrations::write_plan(&dir, "initial", &first).unwrap();
    // A plan cannot be reused after the files it depended on change.
    assert!(migrations::write_plan(&dir, "stale", &first).is_err());
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(BASE)).is_err()); // pending
    migrations::deploy(&mut target, &dir).unwrap();
    target
        .execute_raw("INSERT INTO users(email) VALUES ('keep@example.org')")
        .unwrap();
    assert!(
        migrations::plan(&mut target, &mut shadow, &dir, &schema(BASE))
            .unwrap()
            .is_empty()
    );
    assert!(!exists(&mut shadow, "public.users"));
    let expanded = BASE.replace(
        "  email String @unique",
        "  email String @unique\n  nickname String?\n  age Int @default(18)",
    ) + r#"
model Post {
  id Int @id @default(autoincrement())
  title String
  @@map("posts")
}
"#;
    let planned = migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).unwrap();
    assert!(planned
        .sql()
        .contains("ALTER TABLE \"users\" ADD COLUMN \"nickname\" TEXT"));
    assert!(planned
        .sql()
        .contains("ALTER TABLE \"users\" ADD COLUMN \"age\" INTEGER NOT NULL DEFAULT 18"));
    assert!(planned.sql().contains("CREATE TABLE \"posts\""));
    assert!(!exists(&mut shadow, "public.users"));
    assert!(!exists(&mut target, "public.posts"));
    let initial_file = migrations::load(&dir).unwrap().remove(0);
    let initial_path = dir.join(&initial_file.name).join("migration.sql");
    fs::write(&initial_path, format!("{}\n-- changed", initial_file.sql)).unwrap();
    assert!(migrations::write_plan(&dir, "new", &planned).is_err());
    fs::write(&initial_path, &initial_file.sql).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let barrier = barrier.clone();
            let dir = dir.clone();
            let planned = planned.clone();
            std::thread::spawn(move || {
                barrier.wait();
                migrations::write_plan(&dir, &format!("new{n}"), &planned)
            })
        })
        .collect();
    let writes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(writes.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(writes
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .all(|p| p.join("migration.sql").exists()));
    migrations::deploy(&mut target, &dir).unwrap();
    let rows = target
        .query(&CompiledQuery {
            sql: "SELECT email,nickname,age FROM users".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>("email").unwrap(), "keep@example.org");
    assert_eq!(rows[0].get::<Option<String>>("nickname").unwrap(), None);
    assert_eq!(rows[0].get::<i32>("age").unwrap(), 18);
    assert!(
        migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded))
            .unwrap()
            .is_empty()
    );
    // Column alterations, drop/rename, and unsafe additions fail closed.
    for changed in [
        expanded.replace("  age Int @default(18)", "  age BigInt @default(18)"),
        expanded.replace("  nickname String?\n", ""),
        expanded.replace("  email String @unique", "  renamed String @unique"),
        expanded.replace("  title String", "  title String\n  required String"),
        expanded.replace(
            "  title String",
            "  title String\n  another String? @unique",
        ),
    ] {
        assert!(
            migrations::plan(&mut target, &mut shadow, &dir, &schema(&changed)).is_err(),
            "should reject: {changed}"
        );
    }
    assert!(!exists(&mut shadow, "public.users"));
    // Out-of-band target changes are detected, and never copied to the shadow permanently.
    target
        .execute_raw("ALTER TABLE users ADD COLUMN drift text")
        .unwrap();
    let err = migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded))
        .unwrap_err()
        .to_string();
    assert!(err.contains("differs"), "{err}");
    assert!(!exists(&mut shadow, "public.users"));
    target
        .execute_raw("ALTER TABLE users DROP COLUMN drift")
        .unwrap();
    target
        .execute_raw("CREATE INDEX custom_index ON users (email)")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    target.execute_raw("DROP INDEX custom_index").unwrap();
    target
        .execute_raw("ALTER TABLE users ADD CONSTRAINT custom_check CHECK (age >= 0)")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    target
        .execute_raw("ALTER TABLE users DROP CONSTRAINT custom_check")
        .unwrap();
    target
        .execute_raw("ALTER TABLE users ENABLE ROW LEVEL SECURITY")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    target
        .execute_raw("ALTER TABLE users DISABLE ROW LEVEL SECURITY")
        .unwrap();
    // An occupied shadow database is never reset or modified.
    shadow
        .execute_raw("CREATE TABLE preserve_me (id int)")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    assert!(exists(&mut shadow, "public.preserve_me"));
    shadow.execute_raw("DROP TABLE preserve_me").unwrap();
    shadow
        .execute_raw("CREATE SCHEMA unrelated_user_schema")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    shadow
        .execute_raw("DROP SCHEMA unrelated_user_schema")
        .unwrap();
    shadow
        .execute_raw("CREATE SCHEMA _rustorm_plan_expected")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    shadow
        .execute_raw("DROP SCHEMA _rustorm_plan_expected")
        .unwrap();
    target
        .execute_raw("ALTER SEQUENCE users_id_seq INCREMENT BY 5")
        .unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    target
        .execute_raw("ALTER SEQUENCE users_id_seq INCREMENT BY 1")
        .unwrap();
    target.execute_raw("ALTER TABLE users DROP CONSTRAINT users_email_key; ALTER TABLE users ADD CONSTRAINT users_email_key UNIQUE NULLS NOT DISTINCT (email)").unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&expanded)).is_err());
    target.execute_raw("ALTER TABLE users DROP CONSTRAINT users_email_key; ALTER TABLE users ADD CONSTRAINT users_email_key UNIQUE (email)").unwrap();
    // CLI dry-run and persisted plans exercise argument handling and never touch the shadow.
    let next = expanded.replace(
        "  title String",
        "  title String\n  extra String?\n  tag String @default(\"O'Reilly\\\\path\")",
    );
    fs::write(temp.0.join("schema.rsm"), &next).unwrap();
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let opts = [
        "--schema",
        "schema.rsm",
        "--dir",
        "migrations",
        "--local-port",
        port.as_str(),
        "--local-user",
        user.as_str(),
        "--local-database",
        target_name.as_str(),
        "--shadow-local-database",
        shadow_name.as_str(),
    ];
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rustorm"))
            .current_dir(&temp.0)
            .args(args)
            .output()
            .unwrap()
    };
    let count = migrations::load(&dir).unwrap().len();
    let mut args = vec!["migrate", "plan", "--print"];
    args.extend(opts);
    let print = cli(&args);
    assert!(
        print.status.success(),
        "{}",
        String::from_utf8_lossy(&print.stderr)
    );
    assert!(String::from_utf8_lossy(&print.stdout).contains("ADD COLUMN"));
    assert_eq!(migrations::load(&dir).unwrap().len(), count);
    let mut args = vec!["migrate", "plan", "--name", "next"];
    args.extend(opts);
    let written = cli(&args);
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    assert_eq!(migrations::load(&dir).unwrap().len(), count + 1);
    assert!(!exists(&mut shadow, "public.users"));
    migrations::deploy(&mut target, &dir).unwrap();
    assert!(
        migrations::plan(&mut target, &mut shadow, &dir, &schema(&next))
            .unwrap()
            .is_empty()
    );
    target
        .execute_raw("INSERT INTO posts(title) VALUES ('example')")
        .unwrap();
    let tag = target
        .query(&CompiledQuery {
            sql: "SELECT tag FROM posts".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(tag[0].get::<String>("tag").unwrap(), "O'Reilly\\path");
    // Histories that create additional schemas cannot silently be compared as public-only.
    let unsupported = dir.join("zzzz_external_schema");
    fs::create_dir(&unsupported).unwrap();
    fs::write(
        unsupported.join("migration.sql"),
        "CREATE SCHEMA rustorm_other; CREATE TABLE rustorm_other.t (id int);",
    )
    .unwrap();
    migrations::deploy(&mut target, &dir).unwrap();
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(&next)).is_err());
    assert!(!exists(&mut shadow, "public.users"));
    let namespaces=shadow.query(&CompiledQuery{sql:"SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='rustorm_other') AS present".into(),params:vec![]}).unwrap();
    assert!(!namespaces[0].get::<bool>("present").unwrap());
    drop(shadow);
    drop(target);
    drop(databases);
}

#[test]
#[ignore = "requires disposable PostgreSQL with CREATEDB privilege"]
fn shadow_replay_failure_rolls_back_without_creating_a_plan() {
    let temp = Temp::new();
    let dir = temp.0.join("migrations");
    fs::create_dir(&dir).unwrap();
    let admin_db =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let target_name = format!("rustorm_replay_target_{}", std::process::id());
    let shadow_name = format!("rustorm_replay_shadow_{}", std::process::id());
    let mut databases = Databases {
        admin: connect(&admin_db),
        target: target_name.clone(),
        shadow: shadow_name.clone(),
        target_created: false,
        shadow_created: false,
        extra: None,
    };
    databases
        .admin
        .execute_raw(&format!("CREATE DATABASE \"{target_name}\""))
        .unwrap();
    databases.target_created = true;
    databases
        .admin
        .execute_raw(&format!("CREATE DATABASE \"{shadow_name}\""))
        .unwrap();
    databases.shadow_created = true;
    let mut target = connect(&target_name);
    let mut shadow = connect(&shadow_name);
    let first = dir.join("001_initial");
    fs::create_dir(&first).unwrap();
    fs::write(
        first.join("migration.sql"),
        migrations::initial_sql(&schema(BASE)).unwrap(),
    )
    .unwrap();
    migrations::deploy(&mut target, &dir).unwrap();
    target
        .execute_raw("INSERT INTO users(email) VALUES ('required-data@example.org')")
        .unwrap();
    let second = dir.join("002_data_dependent");
    fs::create_dir(&second).unwrap();
    fs::write(
        second.join("migration.sql"),
        "SELECT 1 / (SELECT count(*) FROM users);",
    )
    .unwrap();
    migrations::deploy(&mut target, &dir).unwrap();
    // The target has a row, but replay on the empty shadow divides by zero.
    assert!(migrations::plan(&mut target, &mut shadow, &dir, &schema(BASE)).is_err());
    assert!(!exists(&mut shadow, "public.users"));
    assert_eq!(migrations::load(&dir).unwrap().len(), 2);
    let rows = target
        .query(&CompiledQuery {
            sql: "SELECT count(*)::bigint AS n FROM users".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<i64>("n").unwrap(), 1);
    drop(shadow);
    drop(target);
    drop(databases);
}
