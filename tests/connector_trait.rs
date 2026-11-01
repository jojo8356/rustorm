use rustorm::connector::{CapabilityMatrix, PgVersionMatrix, Provider, SslMode, TlsConfig};
#[cfg(feature = "postgres")]
use rustorm::connector::mock::MockConnector;
#[cfg(feature = "postgres")]
use rustorm::postgres::{Connector, Connection, Error};
use rustorm::query::{CompiledQuery, Field, Insert, Select};
use rustorm::row::{Row, FromRow, DecodeError};

#[test]
fn provider_parse_and_display() {
    assert_eq!(Provider::parse("postgresql").unwrap(), Provider::Postgresql);
    assert_eq!(Provider::parse("postgres").unwrap(), Provider::Postgresql);
    assert_eq!(Provider::parse("PG").unwrap(), Provider::Postgresql);
    assert_eq!(Provider::parse("Postgresql").unwrap().as_str(), "postgresql");
    assert!(Provider::parse("mysql").is_err());
    assert!(Provider::parse("").is_err());
    assert!(Provider::parse("mongodb").is_err());
}

#[test]
fn sslmode_parse_encrypts_verifies() {
    assert_eq!(SslMode::parse("disable").unwrap(), SslMode::Disable);
    assert_eq!(SslMode::parse("allow").unwrap(), SslMode::Allow);
    assert_eq!(SslMode::parse("prefer").unwrap(), SslMode::Prefer);
    assert_eq!(SslMode::parse("require").unwrap(), SslMode::Require);
    assert_eq!(SslMode::parse("verify-ca").unwrap(), SslMode::VerifyCa);
    assert_eq!(SslMode::parse("verify-full").unwrap(), SslMode::VerifyFull);
    assert_eq!(SslMode::parse("verify_ca").unwrap(), SslMode::VerifyCa);
    assert!(SslMode::parse("unknown").is_err());

    assert!(!SslMode::Disable.encrypts());
    assert!(SslMode::Allow.encrypts());
    assert!(SslMode::Require.encrypts());
    assert!(SslMode::VerifyFull.encrypts());

    assert!(SslMode::VerifyFull.verifies());
    assert!(SslMode::VerifyCa.verifies());
    assert!(!SslMode::Require.verifies());
    assert!(!SslMode::Disable.verifies());

    assert_eq!(SslMode::Disable.as_param_str(), "disable");
    assert_eq!(SslMode::VerifyFull.as_param_str(), "verify-full");
}

#[test]
fn tls_config_defaults_and_builders() {
    let full = TlsConfig::verify_full();
    assert_eq!(full.sslmode, SslMode::VerifyFull);
    assert!(full.verify_hostname);
    assert!(full.is_production_ready());
    assert!(full.requires_verification());

    let dis = TlsConfig::disable_for_local();
    assert_eq!(dis.sslmode, SslMode::Disable);
    assert!(!dis.verify_hostname);
    assert!(!dis.is_production_ready());
    assert!(!dis.requires_verification());

    let req = TlsConfig::require();
    assert_eq!(req.sslmode, SslMode::Require);
    assert!(req.is_production_ready());
    assert!(!req.requires_verification());

    let with_ca = TlsConfig::verify_full().with_rootcert("/tmp/ca.crt");
    assert_eq!(with_ca.sslrootcert.as_deref(), Some("/tmp/ca.crt"));
    assert_eq!(with_ca.sslmode, SslMode::VerifyFull);

    let def = TlsConfig::default();
    assert_eq!(def.sslmode, SslMode::VerifyFull);
}

#[test]
fn tls_from_query_and_url() {
    let cfg = TlsConfig::from_query_str("sslmode=disable").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Disable);
    assert!(!cfg.verify_hostname);

    let cfg = TlsConfig::from_query_str("sslmode=require&sslrootcert=/ca.crt&sslcert=/c.crt&sslkey=/k.key").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Require);
    assert_eq!(cfg.sslrootcert.as_deref(), Some("/ca.crt"));
    assert_eq!(cfg.sslcert.as_deref(), Some("/c.crt"));
    assert_eq!(cfg.sslkey.as_deref(), Some("/k.key"));

    let cfg = TlsConfig::from_query_str("sslmode=verify-ca").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyCa);

    let cfg = TlsConfig::from_url("postgres://user@host/db?sslmode=verify-full").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);

    let cfg = TlsConfig::from_url("postgres://user@host/db?sslmode=require").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Require);

    let cfg = TlsConfig::from_url("postgres://user@host/db").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);

    let cfg = TlsConfig::from_url("host=localhost dbname=test sslmode=disable").unwrap();
    // from_url parses ? only, so host style without ? should default to verify-full
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);

    assert!(TlsConfig::from_query_str("sslmode=invalid").is_err());
}

#[test]
fn tls_to_params() {
    let cfg = TlsConfig::verify_full();
    let params = cfg.to_params();
    assert!(params.iter().any(|(k, v)| *k == "sslmode" && v == "verify-full"));

    let cfg = TlsConfig::require().with_rootcert("/tmp/ca.crt");
    let params = cfg.to_params();
    assert!(params.iter().any(|(k, _)| *k == "sslmode"));
    assert!(params.iter().any(|(k, v)| *k == "sslrootcert" && v == "/tmp/ca.crt"));

    let cfg = TlsConfig::disable_for_local();
    let params = cfg.to_params();
    assert_eq!(params[0].1, "disable");
}

#[test]
fn capability_matrix_values() {
    let pg = CapabilityMatrix::postgres();
    assert_eq!(pg.provider, Provider::Postgresql);
    assert!(pg.transactions);
    assert!(pg.json);
    assert!(pg.enums);
    assert!(pg.views);
    assert!(pg.composite_types);
    assert!(pg.tls);
    assert!(pg.pool);
    assert!(pg.savepoints);

    let cock = CapabilityMatrix::cockroach();
    assert!(cock.transactions);
    assert!(!cock.enums);
    assert!(!cock.views);

    let map = pg.as_map();
    assert_eq!(map["transactions"], true);
    assert_eq!(map["tls"], true);
    assert_eq!(map["savepoints"], true);
    assert_eq!(pg.is_supported("transactions"), true);
    // is_supported for unknown returns false
    assert_eq!(pg.is_supported("unknown_feature"), false);
}

#[test]
fn pg_version_matrix_formal() {
    let m = PgVersionMatrix::documented();
    assert!(m.is_green());
    assert!(m.ac_conn_01_verified());
    assert_eq!(m.versions.len(), 2);
    assert_eq!(m.versions[0].version, "16");
    assert_eq!(m.versions[1].version, "18");
    assert_eq!(m.versions[0].major, 16);
    assert_eq!(m.versions[1].major, 18);
    assert_eq!(m.versions[0].ci_image, "postgres:16");
    assert_eq!(m.versions[1].ci_image, "postgres:18");
    for v in &m.versions {
        assert!(v.tested);
        assert!(v.ddl_identical);
        assert!(v.crud_identical);
        assert!(v.tx_identical);
    }
}

#[test]
fn validate_provider_helpers() {
    assert!(rustorm::connector::validate_provider("postgresql").is_ok());
    assert!(rustorm::connector::validate_provider("postgres").is_ok());
    assert!(rustorm::connector::validate_provider("pg").is_ok());
    assert!(rustorm::connector::validate_provider("mysql").is_err());
    assert!(rustorm::connector::validate_provider("").is_err());

    // schema validation
    let schema_ok = rustorm::schema::parse("datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") } model M { id Int @id }").unwrap();
    assert!(rustorm::connector::validate_schema_provider(&schema_ok).is_ok());
}

#[cfg(feature = "postgres")]
#[test]
fn trait_has_two_impls_mock_and_connection() {
    // This test proves the Connector trait compiles with two impls
    fn assert_connector<T: Connector>(_t: &T) {}
    let mock = MockConnector::new();
    assert_connector(&mock);

    // Connection type also implements Connector — we don't instantiate without PG, but type checks
    fn check_connection_is_connector() where Connection: Connector {}
    check_connection_is_connector();
}

#[cfg(feature = "postgres")]
#[test]
fn trait_object_dyn_connector() {
    // Prove trait objects can hold either impl
    let mock = MockConnector::new();
    let mut boxed: Box<dyn Connector> = Box::new(mock);
    let q = CompiledQuery { sql: "SELECT 1".into(), params: vec![] };
    let res = boxed.query(&q);
    assert!(res.is_ok());
    assert_eq!(boxed.execute(&q).unwrap(), 0);
}

#[cfg(feature = "postgres")]
#[test]
fn mock_connector_behavior() {
    let mut mock = MockConnector::new();
    assert_eq!(mock.provider(), Provider::Postgresql);
    assert_eq!(mock.tls_config().sslmode, SslMode::VerifyFull);

    let q1 = CompiledQuery { sql: "SELECT 1".into(), params: vec![] };
    assert!(mock.query(&q1).is_ok());
    assert_eq!(mock.executed.len(), 1);
    assert_eq!(mock.executed[0], "SELECT 1");

    let q2 = CompiledQuery { sql: "INSERT INTO t VALUES (1)".into(), params: vec![] };
    assert_eq!(mock.execute(&q2).unwrap(), 0);
    assert_eq!(mock.executed.len(), 2);

    let mut mock_fail = MockConnector::new().fail_next("boom");
    let err = mock_fail.query(&q1).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)));

    let mock_tls = MockConnector::new().with_tls(TlsConfig::disable_for_local());
    assert_eq!(mock_tls.tls_config().sslmode, SslMode::Disable);
}

#[cfg(feature = "postgres")]
#[test]
fn mock_connector_begin_error() {
    let mut mock = MockConnector::new();
    let err = mock.begin().unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)));
}

#[test]
fn provider_postgresql_only() {
    // Ensure only postgresql is accepted, per P0
    for ok in ["postgresql", "postgres", "pg", "PostgreSQL", "POSTGRES"] {
        assert!(Provider::parse(ok).is_ok(), "should accept {}", ok);
    }
    for bad in ["mysql", "sqlite", "mongodb", "cockroach", "sqlserver", ""] {
        assert!(Provider::parse(bad).is_err(), "should reject {}", bad);
    }
}
