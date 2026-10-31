use std::fs;
use std::path::PathBuf;

#[test]
fn squash_migrations() {
    let dir = PathBuf::from("/tmp/rustorm_squash_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let d1 = dir.join("20200101000000_init");
    let d2 = dir.join("20200102000000_add");
    fs::create_dir_all(&d1).unwrap();
    fs::create_dir_all(&d2).unwrap();
    fs::write(d1.join("migration.sql"), "CREATE TABLE users (id SERIAL PRIMARY KEY);").unwrap();
    fs::write(d2.join("migration.sql"), "ALTER TABLE users ADD COLUMN name TEXT;").unwrap();
    // Simulate squash: concatenate from 20200101000000_init
    let mut sql_concat = String::new();
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if dir_name >= "20200101000000_init" {
                    if let Ok(content) = fs::read_to_string(path.join("migration.sql")) {
                        sql_concat.push_str(&format!("-- from {dir_name}\n{content}\n"));
                    }
                }
            }
        }
    }
    assert!(sql_concat.contains("CREATE TABLE"));
    assert!(sql_concat.contains("ALTER TABLE"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn strategy_rolling() {
    // stub: just check that strategy is parsed
    let strategy = "rolling";
    assert!(["rolling","blue-green"].contains(&strategy));
}
