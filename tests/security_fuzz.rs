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
