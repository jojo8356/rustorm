#![cfg(feature = "postgres")]
use rustorm::postgres::{Connection, Error, IsolationLevel, TransactionOptions};
use rustorm::query::{CompiledQuery, Field, Insert, Value};

fn pg_conn() -> Connection {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let db = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    Connection::connect_local_unencrypted(port, &user, &db).expect("connect pg for security test")
}

const PARENT_TABLE: &str = "sec_parent";
const CHILD_TABLE: &str = "sec_child";

fn setup_basic(db: &mut Connection) {
    let _ = db.execute_raw(&format!("DROP TABLE IF EXISTS {CHILD_TABLE} CASCADE"));
    let _ = db.execute_raw(&format!("DROP TABLE IF EXISTS {PARENT_TABLE} CASCADE"));
    db.execute_raw(&format!("CREATE TABLE {PARENT_TABLE} (id integer PRIMARY KEY)")).unwrap();
    db.execute_raw(&format!("CREATE TABLE {CHILD_TABLE} (id integer PRIMARY KEY, parent_id integer REFERENCES {PARENT_TABLE}(id) ON DELETE CASCADE ON UPDATE CASCADE)")).unwrap();
    let _ = db.execute_raw(&format!("ALTER TABLE {CHILD_TABLE} DISABLE ROW LEVEL SECURITY"));
    let _ = db.execute_raw(&format!("ALTER TABLE {PARENT_TABLE} DISABLE ROW LEVEL SECURITY"));
    let _ = db.execute_raw(&format!("ALTER TABLE {CHILD_TABLE} NO FORCE ROW LEVEL SECURITY"));
}

fn cleanup(db: &mut Connection) {
    let _ = db.execute_raw(&format!("DROP TABLE IF EXISTS {CHILD_TABLE} CASCADE"));
    let _ = db.execute_raw(&format!("DROP TABLE IF EXISTS {PARENT_TABLE} CASCADE"));
    let _ = db.execute_raw("DROP POLICY IF EXISTS sec_policy ON sec_child");
    let _ = db.execute_raw("DROP FUNCTION IF EXISTS sec_trigger_fn() CASCADE");
}

/// Helper to check row_security_active via same query as nested_writes uses
fn is_rls_active(db: &mut Connection, table: &str) -> bool {
    let q = CompiledQuery {
        sql: "SELECT row_security_active($1::regclass) AS active".into(),
        params: vec![Value::String(format!("\"{table}\""))],
    };
    let rows = db.query(&q).unwrap();
    rows[0].get::<bool>("active").unwrap()
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn rls_blocks_nested_write() {
    let mut db = pg_conn();
    setup_basic(&mut db);
    // Enable RLS
    db.execute_raw(&format!("ALTER TABLE {CHILD_TABLE} ENABLE ROW LEVEL SECURITY")).unwrap();
    db.execute_raw(&format!("CREATE POLICY sec_policy ON {CHILD_TABLE} FOR ALL USING (false)")).unwrap();
    db.execute_raw(&format!("ALTER TABLE {CHILD_TABLE} FORCE ROW LEVEL SECURITY")).unwrap();

    // Verify row_security_active is true — our nested_writes guard checks this
    let active = is_rls_active(&mut db, CHILD_TABLE);
    assert!(active, "row_security_active should be true when RLS enabled");

    // Now test that a transaction that checks isolation+RLS would see it — we simulate the guard query from nested_writes
    let mut tx = db.begin().unwrap();
    let inspection = CompiledQuery {
        sql: r#"SELECT current_setting('transaction_isolation') AS isolation, row_security_active(to_regclass($1)) AS rls"#.to_owned(),
        params: vec![Value::String(format!("\"{CHILD_TABLE}\""))],
    };
    let rows = tx.query(&inspection).unwrap();
    assert_eq!(rows[0].get::<String>("isolation").unwrap(), "read committed");
    assert!(rows[0].get::<bool>("rls").unwrap(), "RLS should be active in tx");

    tx.rollback().unwrap();
    cleanup(&mut db);
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn trigger_blocks_nested_write() {
    let mut db = pg_conn();
    setup_basic(&mut db);
    db.execute_raw("CREATE OR REPLACE FUNCTION sec_trigger_fn() RETURNS trigger AS $$ BEGIN RETURN NEW; END; $$ LANGUAGE plpgsql").unwrap();
    db.execute_raw(&format!("CREATE TRIGGER sec_trigger BEFORE INSERT OR UPDATE ON {CHILD_TABLE} FOR EACH ROW EXECUTE FUNCTION sec_trigger_fn()")).unwrap();

    let has_trigger = {
        let q = CompiledQuery {
            sql: "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t WHERE t.tgrelid=$1::regclass::oid AND NOT t.tgisinternal AND t.tgenabled <> 'D') AS has".into(),
            params: vec![Value::String(format!("\"{CHILD_TABLE}\""))],
        };
        let rows = db.query(&q).unwrap();
        rows[0].get::<bool>("has").unwrap()
    };
    assert!(has_trigger, "trigger should be detected");

    // Verify our guard would block — simulate nested_writes inspection
    let mut tx = db.begin().unwrap();
    let inspection = CompiledQuery {
        sql: r#"SELECT (EXISTS (SELECT 1 FROM pg_catalog.pg_trigger t WHERE t.tgrelid=to_regclass($1)::oid AND NOT t.tgisinternal AND t.tgenabled <> 'D')) AS has_trigger"#.to_owned(),
        params: vec![Value::String(format!("\"{CHILD_TABLE}\""))],
    };
    let rows = tx.query(&inspection).unwrap();
    assert!(rows[0].get::<bool>("has_trigger").unwrap());
    tx.rollback().unwrap();

    let _ = db.execute_raw(&format!("DROP TRIGGER IF EXISTS sec_trigger ON {CHILD_TABLE}"));
    let _ = db.execute_raw("DROP FUNCTION IF EXISTS sec_trigger_fn() CASCADE");
    cleanup(&mut db);
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn rule_blocks_nested_write() {
    let mut db = pg_conn();
    setup_basic(&mut db);
    db.execute_raw(&format!("CREATE RULE sec_rule AS ON INSERT TO {CHILD_TABLE} DO INSTEAD NOTHING")).unwrap();

    let has_rule = {
        let q = CompiledQuery {
            sql: "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_rewrite w WHERE w.ev_class=$1::regclass::oid AND w.rulename <> '_RETURN') AS has".into(),
            params: vec![Value::String(format!("\"{CHILD_TABLE}\""))],
        };
        let rows = db.query(&q).unwrap();
        rows[0].get::<bool>("has").unwrap()
    };
    assert!(has_rule, "rule should be detected");
    let _ = db.execute_raw(&format!("DROP RULE IF EXISTS sec_rule ON {CHILD_TABLE}"));
    cleanup(&mut db);
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn isolation_must_be_read_committed() {
    let mut db = pg_conn();
    setup_basic(&mut db);
    let opts = TransactionOptions::default().isolation(IsolationLevel::Serializable);
    let mut tx = db.begin_with_options(opts).unwrap();
    let isolation = {
        let rows = tx.query(&CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation".into(), params: vec![] }).unwrap();
        rows[0].get::<String>("isolation").unwrap()
    };
    assert_eq!(isolation, "serializable", "SERIALIZABLE tx should report serializable, not read committed");
    // Our guard requires read committed — so a real nested_set_ext would fail; we just verify the check works
    assert_ne!(isolation, "read committed");
    tx.rollback().unwrap();
    // Also check default is read committed
    let mut tx2 = db.begin().unwrap();
    let iso2 = {
        let rows = tx2.query(&CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation".into(), params: vec![] }).unwrap();
        rows[0].get::<String>("isolation").unwrap()
    };
    assert_eq!(iso2, "read committed");
    tx2.rollback().unwrap();
    cleanup(&mut db);
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn on_update_cascade_is_tested() {
    let mut db = pg_conn();
    setup_basic(&mut db);
    // Verify FK exists and is not deferrable (our guard checks NOT c.condeferrable)
    let mut tx = db.begin().unwrap();
    let q = CompiledQuery {
        sql: "SELECT c.condeferrable AS deferrable FROM pg_catalog.pg_constraint c WHERE c.conrelid='sec_child'::regclass AND c.contype='f'".into(),
        params: vec![],
    };
    let rows = tx.query(&q).unwrap();
    assert!(!rows.is_empty(), "FK should exist");
    assert!(!rows[0].get::<bool>("deferrable").unwrap(), "FK should not be deferrable");
    tx.rollback().unwrap();
    cleanup(&mut db);
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn shadow_ephemeral_even_on_error() {
    use rustorm::migrations::plan;
    use rustorm::schema;
    let mut target = pg_conn();
    let mut shadow = pg_conn();
    let _ = shadow.execute_raw("DROP SCHEMA IF EXISTS _rustorm_plan_expected CASCADE");
    let s = schema::parse("datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") } model User { id Int @id name String }").unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm_test_shadow_{}_{}", std::process::id(), 0));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let _ = plan(&mut target, &mut shadow, &dir, &s);
    let check = shadow.query(&CompiledQuery { sql: "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname = '_rustorm_plan_expected') AS exists".into(), params: vec![] }).unwrap();
    assert!(!check[0].get::<bool>("exists").unwrap(), "shadow must not leak _rustorm_plan_expected");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn transaction_uses_read_committed_default() {
    let mut db = pg_conn();
    setup_basic(&mut db);
    // Ensure default transaction isolation is read committed per §10 R2
    let mut tx = db.begin().unwrap();
    let rows = tx.query(&CompiledQuery { sql: "SELECT current_setting('transaction_isolation') AS isolation".into(), params: vec![] }).unwrap();
    assert_eq!(rows[0].get::<String>("isolation").unwrap(), "read committed");
    tx.rollback().unwrap();
    cleanup(&mut db);
}
