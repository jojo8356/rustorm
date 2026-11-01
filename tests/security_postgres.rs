#![cfg(feature = "postgres")]
use rustorm::postgres::Connection;
use rustorm::query::{CompiledQuery, Value};

fn pg_conn() -> Connection {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let db = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    Connection::connect_local_unencrypted(port, &user, &db).expect("connect pg for security test")
}

#[test]
#[ignore = "requires PG"]
fn rls_placeholder() {
    let mut db = pg_conn();
    let rows = db.query(&CompiledQuery { sql: "SELECT 1 AS one".into(), params: vec![] }).unwrap();
    assert!(!rows.is_empty());
}
