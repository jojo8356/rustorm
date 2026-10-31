use rustorm::connectors_p2::ConnectorKind;
use rustorm::schema::parse;

#[test]
fn connector_capabilities() {
    let caps = ConnectorKind::Cockroach.capabilities();
    assert!(caps.transactions);
    assert!(!caps.enums);
    assert!(ConnectorKind::Postgres.capabilities().composite_types);
}

#[test]
fn cockroach_sql_generation() {
    let kind = ConnectorKind::parse("cockroach").unwrap();
    assert_eq!(kind, ConnectorKind::Cockroach);
    assert!(kind.is_supported("transactions"));
    assert!(!kind.is_supported("enums"));
}

#[test]
fn unsupported_connector_error() {
    assert!(ConnectorKind::parse("oracle").is_err());
    let schema = parse(r#"datasource db { provider = "postgresql" url = env("DATABASE_URL") } generator client { provider = "rustorm-client" output = "../src/generated/rustorm" } view V { id Int }"#).unwrap();
    let err = rustorm::connectors_p2::check_schema_compatibility(ConnectorKind::Cockroach, &schema);
    assert!(err.is_err());
}
