#[test]
fn studio_help_exists() {
    let main = std::fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("studio") && main.contains("--port"));
}

#[test]
fn sql_import_generates_rsm() {
    let sql = "CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT, email TEXT);";
    let rsm = rustorm::migrations::sql_to_rsm(sql).unwrap();
    assert!(rsm.contains("model User") || rsm.contains("model user") || rsm.contains("User"));
    assert!(rsm.contains("id Int") || rsm.contains("id"));
    assert!(rsm.contains("@@map"));
}

#[test]
fn sql_import_unknown_type_warning() {
    let sql = "CREATE TABLE t (id weirdtype PRIMARY KEY);";
    let rsm = rustorm::migrations::sql_to_rsm(sql).unwrap();
    assert!(rsm.contains("model"));
}
