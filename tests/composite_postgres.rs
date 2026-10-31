#![cfg(feature = "postgres")]
use rustorm::{migrations, postgres::Connection, query::CompiledQuery, schema};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model Post {
 id Int @id
 categories PostCategory[]
 @@map("posts")
}
model Category {
 id String @id
 posts PostCategory[]
 @@map("categories")
}
model PostCategory {
 postId Int @map("post_id")
 categoryId String @map("category_id")
 assignedBy String @map("assigned_by")
 post Post @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 category Category @relation(fields: [categoryId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 @@id([postId, categoryId])
 @@map("post_categories")
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
fn explicit_association_composite_key_migration_and_client() {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    let name = format!("rustorm_composite_{}", std::process::id());
    admin
        .execute_raw(&format!("CREATE DATABASE \"{name}\""))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-composite-{}", std::process::id()));
    let guard = Cleanup {
        admin,
        name: name.clone(),
        dir: dir.clone(),
    };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    assert_eq!(migrations::deploy(&mut db, &dir).unwrap().len(), 1);
    db.execute_raw("INSERT INTO posts VALUES (1),(2); INSERT INTO categories VALUES ('rust'),('db'); INSERT INTO post_categories VALUES (1,'rust','alice'),(1,'db','bob'),(2,'rust','claire')").unwrap();
    assert!(db
        .execute_raw("INSERT INTO post_categories VALUES (1,'rust','other')")
        .is_err());
    assert!(db
        .execute_raw("INSERT INTO post_categories VALUES (1,'missing','x')")
        .is_err());
    assert!(db
        .execute_raw("INSERT INTO post_categories VALUES (NULL,'rust','x')")
        .is_err());
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT count(*) AS n FROM post_categories".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<i64>("n").unwrap(), 3);
    let scalar_composite = schema::parse("datasource db {\n provider = \"postgresql\"\n}\nmodel A {\n a Int\n b Int\n @@id([a,b])\n}\n").unwrap();
    let mut shadow = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let error = migrations::plan(&mut db, &mut shadow, &dir, &scalar_composite)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("does not yet support composite primary keys"),
        "{error}"
    );
    drop(shadow);
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"),format!("[package]\nname=\"composite_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"),r#"mod generated;
use generated::{post,category,post_category};
use rustorm::query::Filter;
fn main() {
 let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
 let user=std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
 let name=std::env::var("RUSTORM_COMPOSITE_DB").unwrap();
 let mut db=rustorm::postgres::Connection::connect_local_unencrypted(port,&user,&name).unwrap();
 let rows=db.include_many(post::find_many().order_by(post::ID.asc()),post::categories()).unwrap();
 assert_eq!(rows.len(),2); assert_eq!(rows[0].children.len(),2); assert_eq!(rows[1].children.len(),1);
 assert!(rows[0].children.iter().any(|c| c.assigned_by == "alice"));
 let links=db.include_one(post_category::find_many(),post_category::category()).unwrap();
 assert_eq!(links.len(),3); assert!(links.iter().all(|p| p.parent.as_ref().unwrap().id == p.child.category_id));
 let q=post::find_many().where_(post::categories().some(post_category::category().is(category::ID.equals("rust")))).compile().unwrap();
 assert_eq!(db.fetch::<post::Record>(&q).unwrap().len(),2);
 let insertion=post_category::create().set(post_category::POST_ID,2).set(post_category::CATEGORY_ID,"db").set(post_category::ASSIGNED_BY,"test").compile().unwrap();
 assert_eq!(db.execute(&insertion).unwrap(),1);
 let deletion=post_category::delete_many().where_(Filter::and(vec![post_category::POST_ID.equals(2),post_category::CATEGORY_ID.equals("db")])).compile().unwrap();
 assert_eq!(db.execute(&deletion).unwrap(),1);
}
"#).unwrap();
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&consumer)
            .args(["run", "--offline", "--quiet"])
            .env("RUSTORM_COMPOSITE_DB", &name)
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
    db.execute_raw("UPDATE posts SET id=3 WHERE id=1; DELETE FROM categories WHERE id='rust'")
        .unwrap();
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT post_id, category_id FROM post_categories".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i32>("post_id").unwrap(), 3);
    assert_eq!(rows[0].get::<String>("category_id").unwrap(), "db");
    assert_eq!(
        db.query(&CompiledQuery {
            sql: "SELECT count(*) AS n FROM posts".into(),
            params: vec![]
        })
        .unwrap()[0]
            .get::<i64>("n")
            .unwrap(),
        2
    );
    drop(db);
    drop(guard);
}
