#![cfg(feature = "postgres")]
use rustorm::postgres::{AsyncPool, Error, IsolationLevel, Pool, PoolConfig, TransactionOptions};
use std::time::Duration;

#[test]
fn isolation_level_sql() {
    assert_eq!(IsolationLevel::ReadCommitted.sql(), "READ COMMITTED");
    assert_eq!(IsolationLevel::RepeatableRead.sql(), "REPEATABLE READ");
    assert_eq!(IsolationLevel::Serializable.sql(), "SERIALIZABLE");
}

#[test]
fn transaction_options_builder() {
    let opts = TransactionOptions::new()
        .isolation(IsolationLevel::Serializable)
        .timeout(Duration::from_secs(5))
        .max_retries(2)
        .retry(false);
    assert_eq!(opts.isolation, IsolationLevel::Serializable);
    assert_eq!(opts.timeout, Some(Duration::from_secs(5)));
    assert_eq!(opts.max_retries, 2);
    assert!(!opts.retry_on_deadlock);
    // default
    let def = TransactionOptions::default();
    assert_eq!(def.isolation, IsolationLevel::ReadCommitted);
    assert_eq!(def.max_retries, 3);
    assert!(def.retry_on_deadlock);
}

#[test]
fn pool_config_and_metrics() {
    let cfg = PoolConfig::new("postgres://localhost/db", 5)
        .connection_timeout(Duration::from_secs(3))
        .idle_timeout(Some(Duration::from_secs(30)));
    assert_eq!(cfg.max_connections, 5);
    assert_eq!(cfg.connection_timeout, Duration::from_secs(3));
    let pool = Pool::with_config(cfg);
    assert_eq!(pool.max_connections(), 5);
    assert_eq!(pool.connection_timeout(), Duration::from_secs(3));
    let m = pool.metrics();
    assert_eq!(m.max_connections, 5);
    assert_eq!(m.in_use, 0);
    assert_eq!(m.created, 0);
    // cloneable
    let pool2 = pool.clone();
    assert_eq!(pool2.max_connections(), 5);
    // Debug redacts (does not contain url)
    let dbg = format!("{:?}", pool);
    assert!(!dbg.contains("postgres://"));
}

#[test]
fn async_pool_creation_and_metrics() {
    let cfg = PoolConfig::new("postgres://localhost/db", 7);
    let apool = AsyncPool::with_config(cfg);
    assert_eq!(apool.max_connections(), 7);
    let m = apool.metrics();
    assert_eq!(m.max_connections, 7);
    // cloneable
    let _clone = apool.clone();
}

#[test]
fn pool_new_and_cloneable() {
    let pool = Pool::new("postgres://localhost/db".to_string(), 5);
    assert_eq!(pool.max_connections(), 5);
    let pool2 = pool.clone();
    assert_eq!(pool2.max_connections(), 5);
    assert_eq!(pool.metrics().max_connections, 5);
}

#[test]
fn error_sql_state_and_helpers() {
    let uv = Error::UniqueViolation { constraint: Some("users_email_key".into()), table: Some("users".into()), code: Some("23505".into()) };
    assert_eq!(uv.sql_state(), Some("23505"));
    assert_eq!(uv.constraint_name(), Some("users_email_key"));
    assert_eq!(uv.table_name(), Some("users"));
    assert!(uv.is_unique_violation());
    assert!(!uv.is_retryable());
    assert!(!uv.is_foreign_key_violation());

    let fk = Error::ForeignKeyViolation { constraint: Some("posts_user_fk".into()), code: Some("23503".into()) };
    assert_eq!(fk.sql_state(), Some("23503"));
    assert!(fk.is_foreign_key_violation());
    assert_eq!(fk.constraint_name(), Some("posts_user_fk"));

    let nf = Error::NotFound { code: Some("P0002".into()) };
    assert_eq!(nf.sql_state(), Some("P0002"));
    assert!(nf.is_not_found());

    // retryable codes
    let deadlock = Error::Server { code: Some("40P01".into()) };
    assert!(deadlock.is_retryable());
    let serial = Error::Server { code: Some("40001".into()) };
    assert!(serial.is_retryable());
    let other = Error::Server { code: Some("23505".into()) };
    assert!(!other.is_retryable());

    // Display redacts (no secret leak) and contains sqlState
    let display = format!("{}", uv);
    assert!(display.contains("23505"));
    assert!(display.contains("users_email_key"));
    assert!(!format!("{:?}", uv).contains("secret")); // Debug should not leak either (we check not leaking url, but error Display is safe)
}

#[test]
fn error_not_found_and_timeout() {
    let nf = Error::NotFound { code: None };
    assert_eq!(nf.sql_state(), Some("P0002")); // defaults to P0002
    assert!(nf.is_not_found());
    assert_eq!(format!("{}", nf), "record not found (SQLSTATE P0002)");

    let to = Error::Timeout;
    assert_eq!(to.sql_state(), None);
    assert_eq!(format!("{}", to), "operation timed out");

    let cancelled = Error::Cancelled;
    assert_eq!(format!("{}", cancelled), "operation cancelled");

    let pool_to = Error::PoolTimeout;
    assert_eq!(format!("{}", pool_to), "pool timeout: could not acquire connection");
}

#[test]
fn pool_debug_redacts_url() {
    let pool = Pool::new("postgres://user:secret@localhost/db".to_string(), 5);
    let dbg = format!("{:?}", pool);
    assert!(!dbg.contains("secret"));
    assert!(!dbg.contains("postgres://"));
}

#[test]
fn transaction_options_timeout_zero() {
    let opts = TransactionOptions::default().timeout(Duration::from_millis(0));
    assert_eq!(opts.timeout, Some(Duration::from_millis(0)));
}
