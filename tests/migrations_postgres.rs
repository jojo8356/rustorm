#![cfg(feature = "postgres")]
use rustorm::{
    migrations::{self, Error, Resolution},
    postgres::Connection,
    query::{CompiledQuery, Value},
    schema,
};
use std::{fs, path::PathBuf, process::Command};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("rustorm-migration-pg-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn connection() -> Connection {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let db = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    Connection::connect_local_unencrypted(port, &user, &db).unwrap()
}
fn exists(db: &mut Connection, name: &str) -> bool {
    let query = CompiledQuery {
        sql: "SELECT to_regclass($1) IS NOT NULL AS present".into(),
        params: vec![Value::String(name.into())],
    };
    db.query(&query).unwrap()[0].get("present").unwrap()
}
fn write(dir: &std::path::Path, name: &str, sql: &str) -> PathBuf {
    let p = dir.join(name);
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("migration.sql"), sql).unwrap();
    p
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server; drops _rustorm_migrations"]
fn migrations_lifecycle_against_real_postgres() {
    let temp = Temp::new();
    let dir = temp.0.join("migrations");
    fs::create_dir(&dir).unwrap();
    let mut db = connection();
    db.execute_raw("DROP TABLE IF EXISTS public._rustorm_migrations, public.rustorm_mig_a, public.rustorm_mig_b, public.rustorm_mig_c, public.rustorm_mig_rollback CASCADE").unwrap();
    // Never commandeer an unrelated table with Rustorm's reserved name.
    db.execute_raw("CREATE TABLE public._rustorm_migrations (id int)")
        .unwrap();
    assert!(migrations::status(&mut db, &dir).is_err());
    assert!(migrations::deploy(&mut db, &dir).is_err());
    db.execute_raw("DROP TABLE public._rustorm_migrations")
        .unwrap();
    let sql="CREATE TABLE rustorm_mig_a (id integer PRIMARY KEY); INSERT INTO rustorm_mig_a VALUES (1);";
    let first = write(&dir, "001_initial", sql);
    db.execute_raw("BEGIN").unwrap();
    assert!(migrations::status(&mut db, &dir).is_err());
    assert!(migrations::deploy(&mut db, &dir).is_err());
    db.execute_raw("ROLLBACK").unwrap();
    let before = migrations::status(&mut db, &dir).unwrap();
    assert_eq!(before.pending, vec!["001_initial"]);
    assert!(!exists(&mut db, "public._rustorm_migrations"));
    assert_eq!(
        migrations::deploy(&mut db, &dir).unwrap(),
        vec!["001_initial"]
    );
    assert!(exists(&mut db, "public.rustorm_mig_a"));
    assert!(migrations::deploy(&mut db, &dir).unwrap().is_empty());
    let up = migrations::status(&mut db, &dir).unwrap();
    assert_eq!(up.applied, vec!["001_initial"]);
    assert!(up.pending.is_empty());
    fs::write(first.join("migration.sql"), format!("{sql}\n-- modified")).unwrap();
    assert!(migrations::status(&mut db, &dir).unwrap().problems[0].contains("checksum"));
    assert!(migrations::deploy(&mut db, &dir).is_err());
    fs::write(first.join("migration.sql"), sql).unwrap();
    // Applied history must be an ordered prefix and cannot disappear from disk.
    fs::rename(&first, dir.join("005_renamed")).unwrap();
    assert!(migrations::deploy(&mut db, &dir).is_err());
    fs::rename(dir.join("005_renamed"), &first).unwrap();
    write(&dir, "000_inserted_before", "SELECT 1;");
    assert!(migrations::deploy(&mut db, &dir).is_err());
    fs::remove_dir_all(dir.join("000_inserted_before")).unwrap();
    // The per-database advisory lock excludes concurrent operators.
    let mut other = connection();
    let lock = CompiledQuery {
        sql: "SELECT pg_advisory_lock($1)".into(),
        params: vec![Value::BigInt(0x5255_5354_4f52_4d)],
    };
    other.query(&lock).unwrap();
    assert!(matches!(
        migrations::deploy(&mut db, &dir),
        Err(Error::Locked)
    ));
    let unlock = CompiledQuery {
        sql: "SELECT pg_advisory_unlock($1)".into(),
        params: lock.params.clone(),
    };
    other.query(&unlock).unwrap();
    // A failed second statement rolls back the first, persists failure and blocks retries.
    let second = write(
        &dir,
        "002_failure",
        "CREATE TABLE rustorm_mig_rollback (id int); INSERT INTO missing_rustorm_table VALUES (1);",
    );
    assert!(migrations::deploy(&mut db, &dir).is_err());
    assert!(!exists(&mut db, "public.rustorm_mig_rollback"));
    let failed = migrations::status(&mut db, &dir).unwrap();
    assert_eq!(failed.failed.len(), 1);
    assert!(migrations::deploy(&mut db, &dir).is_err());
    assert!(migrations::resolve(&mut db, &dir, "not_the_file", Resolution::RolledBack).is_err());
    migrations::resolve(&mut db, &dir, "002_failure", Resolution::RolledBack).unwrap();
    // The original failed attempt is retained while a corrected file is retried.
    fs::write(
        second.join("migration.sql"),
        "CREATE TABLE rustorm_mig_b (id int);",
    )
    .unwrap();
    assert_eq!(
        migrations::deploy(&mut db, &dir).unwrap(),
        vec!["002_failure"]
    );
    assert!(exists(&mut db, "public.rustorm_mig_b"));
    let attempts = db
        .query(&CompiledQuery {
            sql: "SELECT status FROM public._rustorm_migrations WHERE name = $1 ORDER BY id".into(),
            params: vec![Value::String("002_failure".into())],
        })
        .unwrap();
    assert_eq!(
        attempts
            .iter()
            .map(|r| r.get::<String>("status").unwrap())
            .collect::<Vec<_>>(),
        vec!["rolled_back", "applied"]
    );
    // Marking a failed migration applied is only an operator decision after manual repair.
    write(
        &dir,
        "003_manual",
        "CREATE TABLE rustorm_mig_c (id int); INSERT INTO missing_rustorm_table VALUES (1);",
    );
    assert!(migrations::deploy(&mut db, &dir).is_err());
    assert!(!exists(&mut db, "public.rustorm_mig_c"));
    db.execute_raw("CREATE TABLE rustorm_mig_c (id int)")
        .unwrap();
    migrations::resolve(&mut db, &dir, "003_manual", Resolution::Applied).unwrap();
    assert!(migrations::deploy(&mut db, &dir).unwrap().is_empty());
    assert!(migrations::status(&mut db, &dir)
        .unwrap()
        .pending
        .is_empty());
    // Empty templates and explicit transaction control fail before recording attempts.
    let fourth = write(&dir, "004_empty", "-- TODO");
    assert!(migrations::deploy(&mut db, &dir).is_err());
    fs::write(
        fourth.join("migration.sql"),
        "BEGIN; CREATE TABLE unwanted (x int); COMMIT;",
    )
    .unwrap();
    assert!(migrations::deploy(&mut db, &dir).is_err());
    assert!(!exists(&mut db, "public.unwanted"));
    fs::remove_dir_all(fourth).unwrap();
    // An interrupted "applying" record blocks deployment until explicit resolution.
    let interrupted = write(
        &dir,
        "004_interrupted",
        "CREATE TABLE rustorm_mig_rollback (id int); SELECT pg_sleep(0.2);",
    );
    let hash = migrations::load(&dir)
        .unwrap()
        .last()
        .unwrap()
        .checksum
        .clone();
    let fake=CompiledQuery { sql: "INSERT INTO public._rustorm_migrations (name,checksum,status) VALUES ($1,$2,'applying')".into(), params: vec![Value::String("004_interrupted".into()),Value::String(hash)] };
    db.execute(&fake).unwrap();
    assert!(migrations::deploy(&mut db, &dir).is_err());
    migrations::resolve(&mut db, &dir, "004_interrupted", Resolution::RolledBack).unwrap();
    // Two deployers racing: at most one applies the file, the other either sees
    // the lock or observes that the migration is already applied.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let dir = dir.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut c = connection();
                barrier.wait();
                migrations::deploy(&mut c, &dir).map_err(|e| e.to_string())
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|v| v
                .as_ref()
                .is_ok_and(|names| names == &vec!["004_interrupted".to_string()]))
            .count(),
        1
    );
    assert!(results
        .iter()
        .all(|r| r.is_ok() || r.as_ref().is_err_and(|e| e.contains("lock"))));
    assert!(migrations::status(&mut db, &dir)
        .unwrap()
        .pending
        .is_empty());
    assert!(exists(&mut db, "public.rustorm_mig_rollback"));
    let rows=db.query(&CompiledQuery{sql:"SELECT count(*)::bigint AS n FROM public._rustorm_migrations WHERE name = $1 AND status = 'applied'".into(),params:vec![Value::String("004_interrupted".into())]}).unwrap();
    assert_eq!(rows[0].get::<i64>("n").unwrap(), 1);
    assert!(interrupted.exists());
    // Exercise CLI create, deploy, status and resolve's confirmation guard.
    db.execute_raw("DROP TABLE public._rustorm_migrations, public.rustorm_mig_a, public.rustorm_mig_b, public.rustorm_mig_c, public.rustorm_mig_rollback").unwrap();
    let project = temp.0.join("project");
    fs::create_dir(&project).unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rustorm"))
            .current_dir(&project)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(cli(&["init"]).status.success());
    let schema_path = project.join("rustorm/schema.rsm");
    let original = fs::read_to_string(&schema_path)
        .unwrap()
        .replace("@@map(\"users\")", "@@map(\"rustorm_mig_a\")");
    fs::write(&schema_path, original).unwrap();
    assert!(cli(&["migrate", "create", "--name", "initial"])
        .status
        .success());
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap_or_else(|_| "5432".into());
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let opts = [
        "--local-port",
        port.as_str(),
        "--local-user",
        user.as_str(),
        "--local-database",
        database.as_str(),
    ];
    let mut args = vec!["migrate", "status"];
    args.extend(opts);
    assert_eq!(cli(&args).status.code(), Some(2));
    let mut args = vec!["migrate", "deploy"];
    args.extend(opts);
    let result = cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut args = vec!["migrate", "status"];
    args.extend(opts);
    assert!(cli(&args).status.success());
    assert!(!cli(&["migrate", "resolve", "--name", "x", "--applied"])
        .status
        .success());
    let columns = db
        .query(&CompiledQuery {
            sql:
                "INSERT INTO rustorm_mig_a (email) VALUES ($1) RETURNING id, email, name, published"
                    .into(),
            params: vec![Value::String("a@example.org".into())],
        })
        .unwrap();
    assert_eq!(columns.len(), 1);
    assert!(columns[0].get::<i32>("id").unwrap() > 0);
    assert_eq!(columns[0].get::<String>("email").unwrap(), "a@example.org");
    assert_eq!(columns[0].get::<Option<String>>("name").unwrap(), None);
    assert!(!columns[0].get::<bool>("published").unwrap());
    let _ = schema::parse(&fs::read_to_string(&schema_path).unwrap()).unwrap();
    db.execute_raw("DROP TABLE public._rustorm_migrations, public.rustorm_mig_a")
        .unwrap();

    // Adopt a pre-existing schema: baseline records only the first migration,
    // never executes its SQL, and preserves pre-existing rows.
    let baseline_dir = temp.0.join("baseline");
    fs::create_dir(&baseline_dir).unwrap();
    let first = write(
        &baseline_dir,
        "001_existing",
        "CREATE TABLE rustorm_mig_a (id int PRIMARY KEY); INSERT INTO rustorm_mig_a VALUES (99);",
    );
    write(
        &baseline_dir,
        "002_next",
        "CREATE TABLE rustorm_mig_b (id int);",
    );
    db.execute_raw(
        "CREATE TABLE rustorm_mig_a (id int PRIMARY KEY); INSERT INTO rustorm_mig_a VALUES (42)",
    )
    .unwrap();
    assert!(migrations::baseline(&mut db, &baseline_dir, "002_next").is_err());
    assert!(!exists(&mut db, "public._rustorm_migrations"));
    assert!(migrations::baseline(&mut db, &baseline_dir, "missing").is_err());
    db.execute_raw("CREATE TABLE public._rustorm_migrations (id int)")
        .unwrap();
    assert!(migrations::baseline(&mut db, &baseline_dir, "001_existing").is_err());
    db.execute_raw("DROP TABLE public._rustorm_migrations")
        .unwrap();
    db.execute_raw("BEGIN").unwrap();
    assert!(migrations::baseline(&mut db, &baseline_dir, "001_existing").is_err());
    db.execute_raw("ROLLBACK").unwrap();
    assert!(!exists(&mut db, "public._rustorm_migrations"));
    fs::write(first.join("migration.sql"), "-- empty").unwrap();
    assert!(migrations::baseline(&mut db, &baseline_dir, "001_existing").is_err());
    assert!(!exists(&mut db, "public._rustorm_migrations"));
    fs::write(
        first.join("migration.sql"),
        "CREATE TABLE rustorm_mig_a (id int PRIMARY KEY); INSERT INTO rustorm_mig_a VALUES (99);",
    )
    .unwrap();
    other.query(&lock).unwrap();
    assert!(matches!(
        migrations::baseline(&mut db, &baseline_dir, "001_existing"),
        Err(Error::Locked)
    ));
    other
        .query(&CompiledQuery {
            sql: "SELECT pg_advisory_unlock($1)".into(),
            params: lock.params.clone(),
        })
        .unwrap();
    migrations::baseline(&mut db, &baseline_dir, "001_existing").unwrap();
    assert!(migrations::baseline(&mut db, &baseline_dir, "001_existing").is_err());
    let status = migrations::status(&mut db, &baseline_dir).unwrap();
    assert_eq!(status.applied, vec!["001_existing"]);
    assert_eq!(status.pending, vec!["002_next"]);
    assert_eq!(
        migrations::deploy(&mut db, &baseline_dir).unwrap(),
        vec!["002_next"]
    );
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT id FROM rustorm_mig_a".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i32>("id").unwrap(), 42);
    db.execute_raw("UPDATE public._rustorm_migrations SET status = 'rolled_back'")
        .unwrap();
    assert!(
        migrations::baseline(&mut db, &baseline_dir, "001_existing").is_err(),
        "even rolled-back attempts prohibit baselining"
    );
    db.execute_raw("DROP TABLE public._rustorm_migrations, public.rustorm_mig_b")
        .unwrap();
    // A pair of baseline operations must never record the same file twice.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let dir = baseline_dir.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut db = connection();
                barrier.wait();
                migrations::baseline(&mut db, &dir, "001_existing").map_err(|e| e.to_string())
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|e| e.contains("lock") || e.contains("empty migration history")));
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT count(*)::bigint AS n FROM public._rustorm_migrations".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<i64>("n").unwrap(), 1);
    db.execute_raw("DROP TABLE public._rustorm_migrations, public.rustorm_mig_a")
        .unwrap();

    // CLI confirmation is required before connecting or changing metadata.
    let initial_name = fs::read_dir(project.join("rustorm/migrations"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .into_string()
        .unwrap();
    db.execute_raw(
        &migrations::initial_sql(
            &schema::parse(&fs::read_to_string(&schema_path).unwrap()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut args = vec!["migrate", "baseline", "--name", &initial_name];
    args.extend(opts);
    assert!(!cli(&args).status.success());
    assert!(!exists(&mut db, "public._rustorm_migrations"));
    let mut args = vec!["migrate", "baseline", "--name", &initial_name, "--confirm"];
    args.extend(opts);
    let result = cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("without executing or verifying"));
    let mut args = vec!["migrate", "deploy"];
    args.extend(opts);
    assert!(cli(&args).status.success());
    db.execute_raw("DROP TABLE public._rustorm_migrations, public.rustorm_mig_a")
        .unwrap();
}
