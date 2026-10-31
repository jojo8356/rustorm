#![cfg(feature = "postgres")]
use rustorm::{
    migrations,
    postgres::Connection,
    query::{CompiledQuery, Field, Select},
    relations::{InverseOne, One, Through},
    row::{DecodeError, FromRow, Row},
    schema,
};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model User {
 id Int @id
 profile Profile?
 @@map("users")
}
model Profile {
 id Int @id
 userId Int? @unique @map("user_id")
 user User? @relation(fields: [userId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 @@map("profiles")
}
model Post {
 id Int @id
 categories Category[]
 @@map("posts")
}
model Category {
 id String @id
 posts Post[]
 @@map("categories")
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
#[derive(Debug)]
struct User(i32);
impl FromRow for User {
    fn from_row(r: &Row) -> Result<Self, DecodeError> {
        Ok(Self(r.get("id")?))
    }
}
#[derive(Debug)]
struct Profile(i32, Option<i32>);
impl FromRow for Profile {
    fn from_row(r: &Row) -> Result<Self, DecodeError> {
        Ok(Self(r.get("id")?, r.get("user_id")?))
    }
}
#[derive(Debug)]
struct Post(i32);
impl FromRow for Post {
    fn from_row(r: &Row) -> Result<Self, DecodeError> {
        Ok(Self(r.get("id")?))
    }
}
#[derive(Debug)]
struct Category(String);
impl FromRow for Category {
    fn from_row(r: &Row) -> Result<Self, DecodeError> {
        Ok(Self(r.get("id")?))
    }
}
fn inv() -> InverseOne<User, Profile, i32> {
    InverseOne::new("users", "id", "profiles", "user_id", &["id", "user_id"])
}
fn owner() -> One<Profile, User, i32> {
    One::new("profiles", "user_id", "users", "id", true, &["id"])
}
#[test]
#[ignore = "requires disposable PostgreSQL and CREATEDB privilege"]
fn one_to_one_and_implicit_many_to_many_round_trip() {
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    let name = format!("rustorm_card_{}", std::process::id());
    admin
        .execute_raw(&format!("CREATE DATABASE \"{name}\""))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-card-{}", std::process::id()));
    let guard = Cleanup {
        admin,
        name: name.clone(),
        dir: dir.clone(),
    };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    migrations::deploy(&mut db, &dir).unwrap();
    db.execute_raw("INSERT INTO users VALUES (1),(2),(3); INSERT INTO profiles VALUES (10,1),(11,NULL),(12,NULL); INSERT INTO posts VALUES (1),(2),(3); INSERT INTO categories VALUES ('rust'),('db')").unwrap();
    assert!(db
        .execute_raw("INSERT INTO profiles VALUES (13,1)")
        .is_err());
    assert!(db
        .execute_raw("INSERT INTO profiles VALUES (13,99)")
        .is_err());
    let join = schema::implicit_join_name("Post", "Category");
    db.execute_raw(&format!(
        "INSERT INTO \"{join}\" (a,b) VALUES ('rust',1), ('db',1), ('rust',2)"
    ))
    .unwrap();
    assert!(db
        .execute_raw(&format!("INSERT INTO \"{join}\" VALUES ('rust',1)"))
        .is_err());
    assert!(db
        .execute_raw(&format!("INSERT INTO \"{join}\" VALUES ('missing',1)"))
        .is_err());
    let users = db
        .include_inverse_one(
            Select::new("users", &["id"]).order_by(Field::<i32>::new("users", "id").asc()),
            inv(),
        )
        .unwrap();
    assert_eq!(
        users
            .iter()
            .map(|u| (u.parent.0, u.child.as_ref().map(|p| p.0)))
            .collect::<Vec<_>>(),
        vec![(1, Some(10)), (2, None), (3, None)]
    );
    let profiles = db
        .include_one(
            Select::new("profiles", &["id", "user_id"])
                .order_by(Field::<i32>::new("profiles", "id").asc()),
            owner(),
        )
        .unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(|p| (p.child.1, p.parent.as_ref().map(|x| x.0)))
            .collect::<Vec<_>>(),
        vec![(Some(1), Some(1)), (None, None), (None, None)]
    );
    let uids = |db: &mut Connection, filter| -> Vec<i32> {
        db.query(
            &Select::new("users", &["id"])
                .where_(filter)
                .order_by(Field::<i32>::new("users", "id").asc())
                .compile()
                .unwrap(),
        )
        .unwrap()
        .iter()
        .map(|r| r.get("id").unwrap())
        .collect()
    };
    assert_eq!(
        uids(
            &mut db,
            inv().is(Field::<i32>::new("profiles", "id").equals(10))
        ),
        vec![1]
    );
    assert_eq!(
        uids(
            &mut db,
            inv().is_not(Field::<i32>::new("profiles", "id").equals(10))
        ),
        vec![2, 3]
    );
    // Descriptors normally come from generated code with string literals.
    let static_join: &'static str = Box::leak(join.clone().into_boxed_str());
    let cat = || {
        Through::<Post, Category, i32, String>::new(
            "posts",
            "id",
            "categories",
            "id",
            static_join,
            "b",
            "a",
            &["id"],
        )
    };
    let post = || {
        Through::<Category, Post, String, i32>::new(
            "categories",
            "id",
            "posts",
            "id",
            static_join,
            "a",
            "b",
            &["id"],
        )
    };
    assert!(!db.connect_through(cat(), 1, "rust").unwrap());
    assert!(db.connect_through(cat(), 3, "db").unwrap());
    assert!(!db.connect_through(cat(), 3, "db").unwrap());
    assert!(db.disconnect_through(post(), "db", 3).unwrap());
    assert!(!db.disconnect_through(post(), "db", 3).unwrap());
    assert!(db.connect_through(cat(), 999, "db").is_err());
    let loaded = db
        .include_through(
            Select::new("posts", &["id"]).order_by(Field::<i32>::new("posts", "id").asc()),
            cat(),
        )
        .unwrap();
    assert_eq!(
        loaded
            .iter()
            .map(|x| (x.parent.0, x.children.len()))
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 1), (3, 0)]
    );
    let loaded = db
        .include_through(
            Select::new("categories", &["id"])
                .order_by(Field::<String>::new("categories", "id").asc()),
            post(),
        )
        .unwrap();
    assert_eq!(
        loaded
            .iter()
            .map(|x| (x.parent.0.as_str(), x.children.len()))
            .collect::<Vec<_>>(),
        vec![("db", 1), ("rust", 2)]
    );
    let ids = |db: &mut Connection, filter| -> Vec<i32> {
        db.query(
            &Select::new("posts", &["id"])
                .where_(filter)
                .order_by(Field::<i32>::new("posts", "id").asc())
                .compile()
                .unwrap(),
        )
        .unwrap()
        .iter()
        .map(|r| r.get("id").unwrap())
        .collect()
    };
    let name_field = Field::<String>::new("categories", "id");
    assert_eq!(
        ids(&mut db, cat().some(name_field.equals("rust"))),
        vec![1, 2]
    );
    assert_eq!(ids(&mut db, cat().none(name_field.equals("rust"))), vec![3]);
    assert_eq!(
        ids(&mut db, cat().every(name_field.equals("rust"))),
        vec![2, 3]
    );
    assert_eq!(
        ids(
            &mut db,
            cat().some(post().some(Field::<i32>::new("posts", "id").equals(2)))
        ),
        vec![1, 2]
    );
    let mut tx = db.begin().unwrap();
    assert_eq!(
        tx.include_through(Select::new("posts", &["id"]), cat())
            .unwrap()
            .len(),
        3
    );
    assert!(tx.connect_through(cat(), 3, "db").unwrap());
    assert!(tx.disconnect_through(cat(), 3, "db").unwrap());
    assert!(tx.connect_through(cat(), 3, "db").unwrap());
    assert_eq!(
        tx.include_inverse_one(Select::new("users", &["id"]), inv())
            .unwrap()
            .len(),
        3
    );
    tx.rollback().unwrap();
    assert_eq!(ids(&mut db, cat().some(name_field.equals("db"))), vec![1]);
    db.execute_raw("UPDATE users SET id=4 WHERE id=1").unwrap();
    let updated = db
        .query(&CompiledQuery {
            sql: "SELECT user_id FROM profiles WHERE id=10".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(updated[0].get::<Option<i32>>("user_id").unwrap(), Some(4));
    db.execute_raw("UPDATE categories SET id='rust2' WHERE id='rust'")
        .unwrap();
    assert_eq!(
        ids(&mut db, cat().some(name_field.equals("rust2"))),
        vec![1, 2]
    );
    db.execute_raw("DELETE FROM users WHERE id=4; DELETE FROM categories WHERE id='rust2'")
        .unwrap();
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT user_id FROM profiles WHERE id=10".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<Option<i32>>("user_id").unwrap(), None);
    let rows = db
        .query(&CompiledQuery {
            sql: format!("SELECT COUNT(*) AS n FROM \"{join}\""),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<i64>("n").unwrap(), 1);
    // Compile and run the generated consumer to validate signatures in both directions.
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"),format!("[package]\nname=\"cardinality_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"),r#"mod generated;
fn main() {
 let port=std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap().parse().unwrap();
 let user=std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
 let name=std::env::var("RUSTORM_CARD_DB").unwrap();
 let mut db=rustorm::postgres::Connection::connect_local_unencrypted(port,&user,&name).unwrap();
 assert_eq!(db.include_inverse_one(generated::user::find_many(), generated::user::profile()).unwrap().len(),2);
 assert_eq!(db.include_one(generated::profile::find_many(), generated::profile::user()).unwrap().len(),3);
 assert_eq!(db.include_through(generated::post::find_many(), generated::post::categories()).unwrap().len(),3);
 assert_eq!(db.include_through(generated::category::find_many(), generated::category::posts()).unwrap().len(),1);
 assert!(db.connect_through(generated::post::categories(), 2, "db").unwrap());
 assert!(db.disconnect_through(generated::category::posts(), "db", 2).unwrap());
 let q=generated::post::find_many().where_(generated::post::categories().some(generated::category::ID.equals("db"))).compile().unwrap();
 assert_eq!(db.fetch::<generated::post::Record>(&q).unwrap().len(),1);
}
"#).unwrap();
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&consumer)
            .args(["run", "--offline", "--quiet"])
            .env("RUSTORM_CARD_DB", &name)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(
        consumer.join("src/main.rs"),
        r#"mod generated;
fn wrong(db: &mut rustorm::postgres::Connection) {
 let _ = db.connect_through(generated::post::categories(), 1, 123);
}
fn main() {}
"#,
    )
    .unwrap();
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&consumer)
            .args(["check", "--offline", "--quiet"])
            .output()
            .unwrap();
    assert!(!output.status.success(), "wrong key type was accepted");
    assert!(String::from_utf8_lossy(&output.stderr).contains("IntoFieldValue"));
    drop(db);
    drop(guard);
}
