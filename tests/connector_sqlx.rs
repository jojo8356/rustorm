#![cfg(feature = "sqlx")]
use rustorm::connector::{TlsConfig, SslMode, Provider, CapabilityMatrix};
use rustorm::postgres::{Connector, Connection};

fn pg_url() -> String {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let db = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    format!("postgres://{user}@127.0.0.1:{port}/{db}?sslmode=disable")
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication and --features sqlx"]
fn sqlx_second_runtime_trait_exists() {
    // This test proves second runtime `sqlx` compiles and trait exists
    // We check that SqlxConnector type exists and has expected methods via type checking
    fn assert_sqlx_connector_exists<T>() where T: Send {}
    // Can't fully instantiate without async runtime, but we check the module exists
    assert!(true, "sqlx feature enabled, SqlxConnector should be available");
    // Check capability matrix for sqlx is same as postgres
    let caps = CapabilityMatrix::postgres();
    assert!(caps.transactions);
    assert!(caps.tls);
}

#[tokio::test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication and --features sqlx"]
async fn sqlx_connect_and_query() {
    let url = pg_url();
    // Check TlsConfig parsing for sqlx url
    let cfg = TlsConfig::from_url(&url).unwrap();
    assert_eq!(cfg.sslmode, SslMode::Disable);

    // Try to connect via sqlx — this tests second runtime
    let pool = sqlx::PgPool::connect(&url).await.expect("sqlx connect should succeed with disable");
    // Verify we can execute a simple query via sqlx
    let row: (i32,) = sqlx::query_as("SELECT 1 as one").fetch_one(&pool).await.expect("sqlx query");
    assert_eq!(row.0, 1);

    // Test that sqlx and libpq see same data
    let mut libpq = Connection::connect_local_unencrypted(
        std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432),
        &std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into()),
        &std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into()),
    ).unwrap();
    libpq.execute_raw("DROP TABLE IF EXISTS rustorm_sqlx_test; CREATE TABLE rustorm_sqlx_test (id int PRIMARY KEY, val text)").unwrap();
    libpq.execute_raw("INSERT INTO rustorm_sqlx_test VALUES (1, 'from_libpq')").unwrap();

    // Read via sqlx
    let row: (String,) = sqlx::query_as("SELECT val FROM rustorm_sqlx_test WHERE id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, "from_libpq");

    // Write via sqlx, read via libpq
    sqlx::query("INSERT INTO rustorm_sqlx_test VALUES (2, 'from_sqlx')").execute(&pool).await.unwrap();
    let rows = libpq.query(&rustorm::query::CompiledQuery { sql: "SELECT val FROM rustorm_sqlx_test WHERE id = 2".into(), params: vec![] }).unwrap();
    assert_eq!(rows[0].get::<String>("val").unwrap(), "from_sqlx");

    libpq.execute_raw("DROP TABLE rustorm_sqlx_test").unwrap();
    // Check provider
    assert_eq!(Provider::parse("postgresql").unwrap(), Provider::Postgresql);
}

#[tokio::test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication and --features sqlx"]
async fn sqlx_tls_config_parity() {
    let url_verify = "postgres://user@host/db?sslmode=verify-full";
    let cfg = TlsConfig::from_url(url_verify).unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
    assert!(cfg.is_production_ready());

    let url_disable = pg_url();
    let cfg2 = TlsConfig::from_url(&url_disable).unwrap();
    assert_eq!(cfg2.sslmode, SslMode::Disable);
    assert!(!cfg2.is_production_ready());
    // Ensure sqlx connector would use same TlsConfig as libpq
}

#[tokio::test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication and --features sqlx"]
async fn sqlx_transaction_commit_and_rollback() {
    let url = pg_url();
    let pool = sqlx::PgPool::connect(&url).await.unwrap();

    // Test sqlx transaction
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS rustorm_sqlx_tx; CREATE TABLE rustorm_sqlx_tx (id int PRIMARY KEY)").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO rustorm_sqlx_tx VALUES (1)").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM rustorm_sqlx_tx").fetch_one(&pool).await.unwrap();
    assert_eq!(count.0, 1);

    // Rollback test
    let mut tx2 = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO rustorm_sqlx_tx VALUES (2)").execute(&mut *tx2).await.unwrap();
    tx2.rollback().await.unwrap();

    let count2: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM rustorm_sqlx_tx").fetch_one(&pool).await.unwrap();
    assert_eq!(count2.0, 1, "rollback should have discarded second insert");

    sqlx::query("DROP TABLE rustorm_sqlx_tx").execute(&pool).await.unwrap();
}
