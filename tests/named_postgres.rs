#![cfg(feature = "postgres")]
use rustorm::{migrations, postgres::Connection, schema};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model User {
 id Int @id
 authored Post[] @relation("Written")
 edited Post[] @relation("Edited")
 @@map("users")
}
model Post {
 id Int @id
 authorId Int @map("author_id")
 editorId Int @map("editor_id")
 author User @relation("Written", fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 editor User @relation("Edited", fields: [editorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 @@map("posts")
}
model Node {
 id Int @id
 parentId Int? @map("parent_id")
 parent Node? @relation("Tree", fields: [parentId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 children Node[] @relation("Tree")
 @@map("nodes")
}
"#;
struct Cleanup {
    admin: Connection,
    name: String,
    dir: PathBuf,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self
            .admin
            .execute_raw(&format!("DROP DATABASE \"{}\"", self.name));
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[test]
#[ignore = "requires disposable PostgreSQL and CREATEDB privilege"]
fn named_and_self_relations_run_through_generated_client() {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    let name = format!("rustorm_named_{}", std::process::id());
    admin
        .execute_raw(&format!("CREATE DATABASE \"{name}\""))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-named-{}", std::process::id()));
    let guard = Cleanup {
        admin,
        name: name.clone(),
        dir: dir.clone(),
    };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    migrations::deploy(&mut db, &dir).unwrap();
    db.execute_raw("INSERT INTO users VALUES (1),(2); INSERT INTO posts VALUES (11,1,2),(12,2,1),(13,1,2); INSERT INTO nodes VALUES (1,NULL),(2,1),(3,1),(4,2)").unwrap();
    assert!(db.execute_raw("INSERT INTO nodes VALUES (5,99)").is_err());
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"),format!("[package]\nname=\"named_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"),r#"mod generated;
use generated::{user,post,node};
fn main() {
 let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
 let user=std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
 let name=std::env::var("RUSTORM_NAMED_DB").unwrap();
 let mut db=rustorm::postgres::Connection::connect_local_unencrypted(port,&user,&name).unwrap();
 let authored=db.include_many(user::find_many().order_by(user::ID.asc()),user::authored()).unwrap();
 let edited=db.include_many(user::find_many().order_by(user::ID.asc()),user::edited()).unwrap();
 assert_eq!(authored[0].children.len(),2); assert_eq!(authored[1].children.len(),1);
 assert_eq!(edited[0].children.len(),1); assert_eq!(edited[1].children.len(),2);
 assert_eq!(db.fetch::<user::Record>(&user::find_many().where_(user::edited().some(post::ID.equals(11))).compile().unwrap()).unwrap()[0].id,2);
 let roots=db.include_many(node::find_many().order_by(node::ID.asc()),node::children()).unwrap();
 assert_eq!(roots.iter().map(|x| x.children.len()).collect::<Vec<_>>(),vec![2,1,0,0]);
 let parents=db.include_one(node::find_many().order_by(node::ID.asc()),node::parent()).unwrap();
 assert!(parents[0].parent.is_none()); assert_eq!(parents[3].parent.as_ref().unwrap().id,2);
 let tree=node::find_many().where_(node::children().some(node::children().some(node::ID.equals(4)))).compile().unwrap();
 assert_eq!(db.fetch::<node::Record>(&tree).unwrap()[0].id,1);
 let parent=node::find_many().where_(node::parent().is(node::ID.equals(1))).compile().unwrap();
 assert_eq!(db.fetch::<node::Record>(&parent).unwrap().len(),2);
}
"#).unwrap();
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&consumer)
            .args(["run", "--offline", "--quiet"])
            .env("RUSTORM_NAMED_DB", &name)
            .env("RUSTORM_TEST_DATABASE_PORT", port.to_string())
            .env("RUSTORM_TEST_DATABASE_USER", &user)
            .env("RUSTORM_TEST_DATABASE_NAME", &database)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    db.execute_raw("DELETE FROM nodes WHERE id=1").unwrap();
    let sql = rustorm::query::CompiledQuery {
        sql: "SELECT count(*) AS n FROM nodes WHERE parent_id IS NULL".into(),
        params: vec![],
    };
    assert_eq!(db.query(&sql).unwrap()[0].get::<i64>("n").unwrap(), 2);
    drop(db);
    drop(guard);
}
