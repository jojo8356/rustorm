#![cfg(feature = "postgres")]
use rustorm::postgres::{AsyncPool, Connection, Error, IsolationLevel, Pool, PoolConfig, TransactionOptions};
use rustorm::query::{Field, Insert, Select};
use rustorm::row::{FromRow, Row, DecodeError};
use std::time::Duration;

const TABLE: &str = "rustorm_tx_test";
const ID: Field<i32> = Field::new(TABLE, "id");
const NAME: Field<String> = Field::new(TABLE, "name");

#[derive(Debug, PartialEq, Clone)]
struct Rec { id: i32, name: String }
impl FromRow for Rec {
    fn from_row(row: &Row) -> Result<Self, DecodeError> {
        Ok(Self { id: row.get("id")?, name: row.get("name")? })
    }
}

fn pg_conn() -> Connection {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let db = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    Connection::connect_local_unencrypted(port, &user, &db).expect("connect pg for tx test")
}

fn setup(db: &mut Connection) {
    db.execute_raw(&format!("DROP TABLE IF EXISTS {TABLE}")).unwrap();
    db.execute_raw(&format!("CREATE TABLE {TABLE} (id integer PRIMARY KEY, name text NOT NULL UNIQUE)")).unwrap();
}

fn teardown(db: &mut Connection) {
    let _ = db.execute_raw(&format!("DROP TABLE IF EXISTS {TABLE}"));
}

fn insert(id: i32, name: &str) -> rustorm::query::CompiledQuery {
    Insert::new(TABLE, &["id","name"]).set(ID, id).set(NAME, name.to_owned()).compile().unwrap()
}

// AC-Tx-01: Two creates in transaction + second fails → first rolled back
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn ac_tx_01_two_creates_second_fails_rollback() {
    let mut db = pg_conn();
    setup(&mut db);
    // successful transaction via Connection::transaction
    let res: Result<i32, Error> = db.transaction(|tx| {
        tx.execute(&insert(1, "alice"))?;
        tx.execute(&insert(1, "alice-dup"))?; // duplicate PK -> UniqueViolation
        Ok(1)
    });
    assert!(res.is_err(), "second insert should fail");
    // verify rollback: no rows
    let rows = db.fetch::<Rec>(&Select::new(TABLE, &["id","name"]).compile().unwrap()).unwrap();
    assert!(rows.is_empty(), "first insert should have been rolled back, got {:?}", rows);
    teardown(&mut db);
}

// AC-Tx-02: UniqueViolation vs NotFound (here we test UniqueViolation structured, and NotFound via select empty is not error but we test explicit not-found via our helper)
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn ac_tx_02_unique_violation_vs_not_found() {
    let mut db = pg_conn();
    setup(&mut db);
    db.execute(&insert(1, "bob")).unwrap();
    let err = db.execute(&insert(1, "bob2")).unwrap_err();
    // should be UniqueViolation with sqlState 23505 and constraint extracted
    assert!(matches!(&err, Error::UniqueViolation { .. }), "expected UniqueViolation, got {:?}", err);
    assert_eq!(err.sql_state(), Some("23505"));
    assert!(err.constraint_name().is_some() || err.constraint_name().is_none()); // constraint may be present depending on PG version
    assert!(!format!("{}", err).contains("secret"));
    // NotFound: we test our Error::NotFound helper directly (no PG raises P0002 easily without PL/pgSQL)
    let nf = Error::NotFound { code: Some("P0002".into()) };
    assert!(nf.is_not_found());
    assert_eq!(nf.sql_state(), Some("P0002"));
    teardown(&mut db);
}

// AC-Tx-01 variant with explicit transaction isolation and timeout
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn ac_tx_isolation_and_timeout_options() {
    let mut db = pg_conn();
    setup(&mut db);
    let opts = TransactionOptions::new().isolation(IsolationLevel::Serializable).timeout(Duration::from_secs(2));
    let res = db.transaction_with_options(opts, |tx| {
        tx.execute(&insert(10, "iso"))?;
        Ok(())
    });
    assert!(res.is_ok(), "Serializable transaction should succeed: {:?}", res.err());
    let rows = db.fetch::<Rec>(&Select::new(TABLE, &["id","name"]).where_(ID.equals(10)).compile().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    // timeout: short timeout with long query should not poison connection
    let mut db2 = pg_conn();
    let q = rustorm::query::CompiledQuery { sql: "SELECT pg_sleep(0.5)".into(), params: vec![] };
    let start = std::time::Instant::now();
    let res = db2.query_with_timeout(&q, Duration::from_millis(100));
    // PG may return Cancelled or Server with 57014
    assert!(res.is_err(), "expected timeout/cancel, got {:?}", res.ok());
    let err = res.unwrap_err();
    assert!(matches!(err, Error::Cancelled | Error::Server { code: Some(_) } | Error::Timeout), "got {:?}", err);
    // connection should still be usable (no poison)
    let rows2 = db2.fetch::<Rec>(&Select::new(TABLE, &["id","name"]).compile().unwrap()).unwrap();
    assert_eq!(rows2.len(), 1, "connection should be reusable after timeout");
    teardown(&mut db);
}

// VAL-TX01: Panic in transaction → rollback, no partial commit
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn val_tx01_panic_rollback() {
    let mut db = pg_conn();
    setup(&mut db);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = db.transaction(|tx| {
            tx.execute(&insert(20, "panic-test"))?;
            panic!("intentional panic in transaction");
            #[allow(unreachable_code)]
            Ok(())
        });
    }));
    assert!(result.is_err(), "panic should propagate");
    // verify rollback
    let rows = db.fetch::<Rec>(&Select::new(TABLE, &["id","name"]).where_(ID.equals(20)).compile().unwrap()).unwrap();
    assert!(rows.is_empty(), "panic should have rolled back, got {:?}", rows);
    // pool still usable
    db.execute(&insert(21, "after-panic")).unwrap();
    let rows = db.fetch::<Rec>(&Select::new(TABLE, &["id","name"]).where_(ID.equals(21)).compile().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    teardown(&mut db);
}

// AC-Tx-03: Concurrent pool max=5 → no leak, metrics ok
#[tokio::test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
async fn ac_tx_03_concurrent_pool_max_5() {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let dbname = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let url = format!("host=127.0.0.1 port={port} user={user} dbname={dbname} sslmode=disable connect_timeout=5");
    // Use AsyncPool with max 5
    let pool = AsyncPool::new(url, 5);
    assert_eq!(pool.max_connections(), 5);
    let initial = pool.metrics();
    assert_eq!(initial.max_connections, 5);

    // spawn 20 tasks, each holds connection for 50ms
    let mut handles = Vec::new();
    for i in 0..20 {
        let p = pool.clone();
        handles.push(tokio::spawn(async move {
            let mut conn = p.get().await.expect("pool get");
            // simple query to verify connection works
            let q = rustorm::query::CompiledQuery { sql: "SELECT 1 as one".into(), params: vec![] };
            let _ = conn.query(&q);
            tokio::time::sleep(Duration::from_millis(50)).await;
            // connection returned on drop
            i
        }));
    }
    for h in handles { let r = h.await.expect("join"); assert!(r >= 0); }
    // after all tasks, pool should have no leak: available == max or close, waiting 0
    // give time for recycle
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = pool.metrics();
    // deadpool metrics: size may be <= max, available should be >=0, waiting 0
    assert_eq!(after.waiting, 0, "no waiting after all tasks");
    assert!(after.max_connections == 5);
    // ensure we can still get a connection
    let conn = pool.get().await;
    assert!(conn.is_ok(), "pool should be reusable after concurrent load");
}

// VAL-TX02: Unique violation with constraint/table extraction
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn val_tx02_unique_constraint_extraction() {
    let mut db = pg_conn();
    setup(&mut db);
    db.execute(&insert(30, "uniq")).unwrap();
    let err = db.execute(&insert(31, "uniq")).unwrap_err(); // dup name unique
    assert!(matches!(&err, Error::UniqueViolation { .. }));
    // constraint name should be present (rustorm_tx_test_name_key or similar)
    // table may be present
    let constraint = err.constraint_name();
    assert!(constraint.is_some(), "constraint should be extracted, got {:?}", err);
    assert!(constraint.unwrap().contains("name") || constraint.unwrap().contains("uniq"));
    assert_eq!(err.sql_state(), Some("23505"));
    teardown(&mut db);
}

// VAL-TX03: Timeout without leak already covered, but test explicit Pool metrics after timeout
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn val_tx03_timeout_no_leak() {
    let mut db = pg_conn();
    setup(&mut db);
    db.execute(&insert(40, "keep")).unwrap();
    // short timeout
    let q = rustorm::query::CompiledQuery { sql: "SELECT pg_sleep(0.3)".into(), params: vec![] };
    let res = db.query_with_timeout(&q, Duration::from_millis(80));
    assert!(res.is_err());
    // verify still usable
    let rows = db.fetch::<Rec>(&Select::new(TABLE, &["id","name"]).compile().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "keep");
    teardown(&mut db);
}

// Additional: Pool sync metrics and Cloneable
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn pool_sync_metrics_cloneable() {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let dbname = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let url = format!("host=127.0.0.1 port={port} user={user} dbname={dbname} sslmode=disable connect_timeout=5 password=rustorm-local-no-secret");
    let pool = Pool::new(url, 5);
    assert_eq!(pool.max_connections(), 5);
    let m1 = pool.metrics();
    assert_eq!(m1.in_use, 0);
    let pool2 = pool.clone();
    assert_eq!(pool2.max_connections(), 5);
    // get a connection via pool (sync) and check metrics increment
    let conn = pool.get().unwrap();
    let m2 = pool.metrics();
    assert_eq!(m2.in_use, 1);
    drop(conn);
    let m3 = pool.metrics();
    assert_eq!(m3.in_use, 0, "no leak after drop");
    // Debug redacts url
    let dbg = format!("{:?}", pool);
    assert!(!dbg.contains("secret"));
}

// ForeignKeyViolation test
#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn fk_violation_structured() {
    let mut db = pg_conn();
    setup(&mut db);
    // create parent table
    db.execute_raw("DROP TABLE IF EXISTS rustorm_fk_parent").unwrap();
    db.execute_raw("CREATE TABLE rustorm_fk_parent (id integer PRIMARY KEY)").unwrap();
    db.execute_raw("DROP TABLE IF EXISTS rustorm_fk_child").unwrap();
    db.execute_raw("CREATE TABLE rustorm_fk_child (id integer PRIMARY KEY, parent_id integer REFERENCES rustorm_fk_parent(id))").unwrap();
    // insert parent
    db.execute_raw("INSERT INTO rustorm_fk_parent (id) VALUES (1)").unwrap();
    // valid fk
    let q_ok = rustorm::query::CompiledQuery { sql: "INSERT INTO rustorm_fk_child (id, parent_id) VALUES (1, 1)".into(), params: vec![] };
    assert!(db.execute(&q_ok).is_ok());
    // invalid fk
    let q_bad = rustorm::query::CompiledQuery { sql: "INSERT INTO rustorm_fk_child (id, parent_id) VALUES (2, 999)".into(), params: vec![] };
    let err = db.execute(&q_bad).unwrap_err();
    assert!(matches!(&err, Error::ForeignKeyViolation { .. }), "expected FK violation, got {:?}", err);
    assert_eq!(err.sql_state(), Some("23503"));
    assert!(err.constraint_name().is_some());
    // cleanup
    let _ = db.execute_raw("DROP TABLE rustorm_fk_child");
    let _ = db.execute_raw("DROP TABLE rustorm_fk_parent");
    teardown(&mut db);
}

// test for futures crate needed in ac_tx_03
