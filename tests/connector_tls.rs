use rustorm::connector::{SslMode, TlsConfig, Provider};

#[test]
fn sslmode_all_variants_parse() {
    for (s, expected) in [
        ("disable", SslMode::Disable),
        ("allow", SslMode::Allow),
        ("prefer", SslMode::Prefer),
        ("require", SslMode::Require),
        ("verify-ca", SslMode::VerifyCa),
        ("verify-full", SslMode::VerifyFull),
        ("VERIFY-CA", SslMode::VerifyCa),
        ("Verify-Full", SslMode::VerifyFull),
    ] {
        assert_eq!(SslMode::parse(s).unwrap(), expected, "parse {}", s);
        assert_eq!(SslMode::parse(s).unwrap().as_param_str(), expected.as_param_str());
    }
}

#[test]
fn sslmode_invalid_rejected() {
    for bad in ["", "unknown", "verify", "sslmode", "disable ", " require"] {
        assert!(SslMode::parse(bad).is_err(), "should reject {}", bad);
    }
}

#[test]
fn sslmode_encrypts_semantics() {
    assert!(!SslMode::Disable.encrypts());
    assert!(SslMode::Allow.encrypts());
    assert!(SslMode::Prefer.encrypts());
    assert!(SslMode::Require.encrypts());
    assert!(SslMode::VerifyCa.encrypts());
    assert!(SslMode::VerifyFull.encrypts());
}

#[test]
fn sslmode_verifies_semantics() {
    assert!(!SslMode::Disable.verifies());
    assert!(!SslMode::Allow.verifies());
    assert!(!SslMode::Prefer.verifies());
    assert!(!SslMode::Require.verifies());
    assert!(SslMode::VerifyCa.verifies());
    assert!(SslMode::VerifyFull.verifies());
}

#[test]
fn sslmode_production_default_is_verify_full() {
    assert_eq!(SslMode::production_default(), SslMode::VerifyFull);
    assert_eq!(SslMode::local_development(), SslMode::Disable);
    assert!(SslMode::VerifyFull.verifies());
    assert!(!SslMode::Disable.verifies());
}

#[test]
fn tls_config_verify_full_production() {
    let cfg = TlsConfig::verify_full();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
    assert!(cfg.verify_hostname);
    assert!(cfg.is_production_ready());
    assert!(cfg.requires_verification());
    assert_eq!(cfg.sslrootcert, None);
    let params = cfg.to_params();
    assert!(params.iter().any(|(k, v)| *k == "sslmode" && v == "verify-full"));
}

#[test]
fn tls_config_disable_local_not_production() {
    let cfg = TlsConfig::disable_for_local();
    assert_eq!(cfg.sslmode, SslMode::Disable);
    assert!(!cfg.verify_hostname);
    assert!(!cfg.is_production_ready());
    assert!(!cfg.requires_verification());
    assert!(!cfg.sslmode.encrypts());
}

#[test]
fn tls_config_require_mode() {
    let cfg = TlsConfig::require();
    assert_eq!(cfg.sslmode, SslMode::Require);
    assert!(cfg.is_production_ready());
    assert!(!cfg.requires_verification());
    // Require encrypts but doesn't verify CA
    assert!(cfg.sslmode.encrypts());
    assert!(!cfg.sslmode.verifies());
}

#[test]
fn tls_config_with_rootcert() {
    let cfg = TlsConfig::verify_full().with_rootcert("/path/to/ca.crt");
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
    assert_eq!(cfg.sslrootcert.as_deref(), Some("/path/to/ca.crt"));
    let cfg2 = cfg.clone().with_rootcert("/other/ca.pem");
    assert_eq!(cfg2.sslrootcert.as_deref(), Some("/other/ca.pem"));
}

#[test]
fn tls_from_query_str_simple() {
    let cfg = TlsConfig::from_query_str("sslmode=disable").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Disable);
    let cfg = TlsConfig::from_query_str("sslmode=allow").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Allow);
    let cfg = TlsConfig::from_query_str("sslmode=prefer").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Prefer);
    let cfg = TlsConfig::from_query_str("sslmode=require").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Require);
    let cfg = TlsConfig::from_query_str("sslmode=verify-ca").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyCa);
    let cfg = TlsConfig::from_query_str("sslmode=verify-full").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
}

#[test]
fn tls_from_query_str_with_certs() {
    let cfg = TlsConfig::from_query_str("sslmode=verify-full&sslrootcert=/tmp/ca.crt").unwrap();
    assert_eq!(cfg.sslrootcert.as_deref(), Some("/tmp/ca.crt"));
    let cfg = TlsConfig::from_query_str("sslmode=require&sslrootcert=/ca.crt&sslcert=/c.crt&sslkey=/k.key").unwrap();
    assert_eq!(cfg.sslrootcert.as_deref(), Some("/ca.crt"));
    assert_eq!(cfg.sslcert.as_deref(), Some("/c.crt"));
    assert_eq!(cfg.sslkey.as_deref(), Some("/k.key"));
}

#[test]
fn tls_from_query_str_defaults_to_verify_full() {
    let cfg = TlsConfig::from_query_str("").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
    let cfg = TlsConfig::from_query_str("other=val").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
}

#[test]
fn tls_from_query_str_invalid() {
    assert!(TlsConfig::from_query_str("sslmode=invalid").is_err());
    assert!(TlsConfig::from_query_str("sslmode=").is_err());
}

#[test]
fn tls_from_url_parsing() {
    let cfg = TlsConfig::from_url("postgres://user:pass@host/db?sslmode=disable").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Disable);
    let cfg = TlsConfig::from_url("postgres://user@host/db?sslmode=require").unwrap();
    assert_eq!(cfg.sslmode, SslMode::Require);
    let cfg = TlsConfig::from_url("postgresql://host/db?sslmode=verify-ca&sslrootcert=/ca.pem").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyCa);
    assert_eq!(cfg.sslrootcert.as_deref(), Some("/ca.pem"));
    let cfg = TlsConfig::from_url("postgres://host/db").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
    let cfg = TlsConfig::from_url("host=localhost dbname=test").unwrap();
    assert_eq!(cfg.sslmode, SslMode::VerifyFull);
}

#[test]
fn tls_to_params_roundtrip() {
    for mode in [SslMode::Disable, SslMode::Allow, SslMode::Prefer, SslMode::Require, SslMode::VerifyCa, SslMode::VerifyFull] {
        let cfg = TlsConfig { sslmode: mode, sslrootcert: None, sslcert: None, sslkey: None, verify_hostname: mode == SslMode::VerifyFull };
        let params = cfg.to_params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "sslmode");
        assert_eq!(params[0].1, mode.as_param_str());
        let qs = format!("sslmode={}", mode.as_param_str());
        let parsed = TlsConfig::from_query_str(&qs).unwrap();
        assert_eq!(parsed.sslmode, mode);
    }
}

#[test]
fn tls_config_equality_and_clone() {
    let a = TlsConfig::verify_full();
    let b = TlsConfig::verify_full();
    assert_eq!(a, b);
    let c = a.clone();
    assert_eq!(c, b);
    let d = TlsConfig::disable_for_local();
    assert_ne!(a, d);
}

#[test]
fn provider_tls_matrix() {
    // Ensure postgres provider supports TLS
    use rustorm::connector::CapabilityMatrix;
    let pg = CapabilityMatrix::postgres();
    assert!(pg.tls);
    assert!(pg.transactions);
    // All SslModes are valid for postgres provider
    for mode in [SslMode::Disable, SslMode::VerifyFull] {
        let cfg = TlsConfig { sslmode: mode, sslrootcert: None, sslcert: None, sslkey: None, verify_hostname: false };
        // Provider is always postgresql for P0
        assert_eq!(Provider::parse("postgresql").unwrap(), Provider::Postgresql);
        // TlsConfig should be usable with provider
        assert!(cfg.to_params().len() >= 1);
        assert_eq!(cfg.sslmode.encrypts(), mode.encrypts());
    }
}


