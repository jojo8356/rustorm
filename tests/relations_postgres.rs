#![cfg(feature = "postgres")]
use rustorm::{migrations, postgres::Connection, query::CompiledQuery, schema};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model Post {
 id Int @id
 authorId Int @map("author_id")
 author User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 @@index([authorId])
 @@map("posts")
}
model User {
 id Int @id
 posts Post[]
 @@map("users")
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
fn relational_initial_migration_enforces_fk_and_actions() {
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    let name = format!("rustorm_rel_{}", std::process::id());
    admin
        .execute_raw(&format!("CREATE DATABASE \"{name}\""))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-rel-{}", std::process::id()));
    let guard = Cleanup {
        admin,
        name: name.clone(),
        dir: dir.clone(),
    };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    assert_eq!(migrations::deploy(&mut db, &dir).unwrap().len(), 1);
    let indexes=db.query(&CompiledQuery{sql:"SELECT indexname::text AS name FROM pg_catalog.pg_indexes WHERE schemaname='public' AND tablename='posts' AND indexname LIKE 'rustorm_idx_%'".into(),params:vec![]}).unwrap();
    assert_eq!(indexes.len(), 1);
    assert!(db.execute_raw("INSERT INTO posts VALUES (1, 999)").is_err());
    db.execute_raw("INSERT INTO users VALUES (1); INSERT INTO posts VALUES (1,1)")
        .unwrap();
    assert!(db.execute_raw("DELETE FROM users WHERE id=1").is_err());
    db.execute_raw("UPDATE users SET id=2 WHERE id=1").unwrap();
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT author_id FROM posts WHERE id=1".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<i32>("author_id").unwrap(), 2);
    #[derive(Debug, PartialEq)]
    struct Parent {
        id: i32,
    }
    impl rustorm::row::FromRow for Parent {
        fn from_row(row: &rustorm::row::Row) -> Result<Self, rustorm::row::DecodeError> {
            Ok(Self { id: row.get("id")? })
        }
    }
    #[derive(Debug, PartialEq)]
    struct Child {
        id: i32,
        author_id: i32,
    }
    impl rustorm::row::FromRow for Child {
        fn from_row(row: &rustorm::row::Row) -> Result<Self, rustorm::row::DecodeError> {
            Ok(Self {
                id: row.get("id")?,
                author_id: row.get("author_id")?,
            })
        }
    }
    let many = rustorm::relations::Many::<Parent, Child, i32>::new(
        "users",
        "id",
        "posts",
        "author_id",
        &["id", "author_id"],
    );
    db.execute_raw("INSERT INTO users VALUES (3), (4); INSERT INTO posts VALUES (2,2), (3,3)")
        .unwrap();
    let parents = rustorm::query::Select::new("users", &["id"])
        .order_by(rustorm::query::Field::<i32>::new("users", "id").asc());
    let loaded = db.include_many(parents, many).unwrap();
    assert_eq!(
        loaded
            .iter()
            .map(|p| (p.parent.id, p.children.len()))
            .collect::<Vec<_>>(),
        vec![(2, 2), (3, 1), (4, 0)]
    );
    assert_eq!(
        loaded[0]
            .children
            .iter()
            .map(|c| c.author_id)
            .collect::<Vec<_>>(),
        vec![2, 2]
    );
    let other = rustorm::query::Select::new("posts", &["id", "author_id"]);
    let many = rustorm::relations::Many::<Parent, Child, i32>::new(
        "users",
        "id",
        "posts",
        "author_id",
        &["id", "author_id"],
    );
    assert!(db.include_many(other, many).is_err());
    let relation = || {
        rustorm::relations::Many::<Parent, Child, i32>::new(
            "users",
            "id",
            "posts",
            "author_id",
            &["id", "author_id"],
        )
    };
    let user_id = rustorm::query::Field::<i32>::new("users", "id");
    let post_id = rustorm::query::Field::<i32>::new("posts", "id");
    let ids = |db: &mut Connection, filter| {
        let q = rustorm::query::Select::new("users", &["id"])
            .where_(filter)
            .order_by(user_id.asc())
            .compile()
            .unwrap();
        db.query(&q)
            .unwrap()
            .iter()
            .map(|r| r.get::<i32>("id").unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&mut db, relation().some(post_id.gte(2))), vec![2, 3]);
    assert_eq!(ids(&mut db, relation().none(post_id.equals(1))), vec![3, 4]);
    assert_eq!(ids(&mut db, relation().every(post_id.gte(2))), vec![3, 4]);
    db.execute_raw(
        "ALTER TABLE posts ADD COLUMN score integer; UPDATE posts SET score=1 WHERE id IN (2,3)",
    )
    .unwrap();
    let score = rustorm::query::Field::<Option<i32>>::new("posts", "score");
    assert_eq!(
        ids(&mut db, relation().every(score.equals_some(1))),
        vec![3, 4]
    );
    assert_eq!(ids(&mut db, relation().none(score.equals_some(1))), vec![4]);
    let mut tx = db.begin().unwrap();
    let q = rustorm::query::Update::new("users")
        .increment(user_id, 10)
        .where_(relation().some(post_id.equals(1)))
        .compile()
        .unwrap();
    assert_eq!(tx.execute(&q).unwrap(), 1);
    tx.rollback().unwrap();
    let many = rustorm::relations::Many::<Parent, Child, i32>::new(
        "users",
        "id",
        "posts",
        "author_id",
        &["id", "author_id"],
    );
    let mut tx = db.begin().unwrap();
    let within = tx
        .include_many(rustorm::query::Select::new("users", &["id"]), many)
        .unwrap();
    assert_eq!(within.len(), 3);
    tx.rollback().unwrap();
    let one = rustorm::relations::One::<Child, Parent, i32>::new(
        "posts",
        "author_id",
        "users",
        "id",
        false,
        &["id"],
    );
    let children =
        rustorm::query::Select::new("posts", &["id", "author_id"]).order_by(post_id.asc());
    let loaded = db.include_one(children, one).unwrap();
    assert_eq!(
        loaded
            .iter()
            .map(|x| (x.child.id, x.parent.as_ref().unwrap().id))
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 2), (3, 3)]
    );
    let one = rustorm::relations::One::<Child, Parent, i32>::new(
        "posts",
        "author_id",
        "users",
        "id",
        false,
        &["id"],
    );
    let mut tx = db.begin().unwrap();
    assert_eq!(
        tx.include_one(
            rustorm::query::Select::new("posts", &["id", "author_id"]),
            one
        )
        .unwrap()
        .len(),
        3
    );
    tx.rollback().unwrap();
    // Compile and execute a consumer using the generated, statically typed descriptor.
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"), format!("[package]\nname=\"relation_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"), r#"mod generated;
fn main() {
 let port=std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap().parse().unwrap();
 let user=std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
 let dbname=std::env::var("RUSTORM_REL_TEST_DB").unwrap();
 let mut db=rustorm::postgres::Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();
 let filtered=generated::user::find_many().where_(generated::user::posts().some(generated::post::ID.gte(2))).compile().unwrap();
 assert_eq!(db.fetch::<generated::user::Record>(&filtered).unwrap().len(),2);
 let rows=db.include_many(generated::user::find_many().order_by(generated::user::ID.asc()), generated::user::posts()).unwrap();
 assert_eq!(rows.len(),3); assert_eq!(rows[0].parent.id,2); assert_eq!(rows[0].children.len(),2); assert!(rows[2].children.is_empty());
 let inverse=db.include_one(generated::post::find_many().order_by(generated::post::ID.asc()), generated::post::author()).unwrap();
 assert_eq!(inverse.len(),3); assert_eq!(inverse[0].parent.as_ref().unwrap().id,2); assert_eq!(inverse[2].parent.as_ref().unwrap().id,3);
}
"#).unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .current_dir(&consumer)
        .args(["run", "--offline", "--quiet"])
        .env("RUSTORM_REL_TEST_DB", &name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // A text FK may compare equal under database collation while its bytes differ.
    // The loader must group by the canonical parent key returned by PostgreSQL.
    db.execute_raw("CREATE COLLATION rustorm_test_ci (provider = icu, locale = 'und-u-ks-level2', deterministic = false); CREATE TABLE r_parent (id text COLLATE rustorm_test_ci PRIMARY KEY); CREATE TABLE r_child (id int PRIMARY KEY, parent_id text COLLATE rustorm_test_ci REFERENCES r_parent(id)); INSERT INTO r_parent VALUES ('Alpha'); INSERT INTO r_child VALUES (1, 'alpha')").unwrap();
    struct TextParent(String);
    impl rustorm::row::FromRow for TextParent {
        fn from_row(row: &rustorm::row::Row) -> Result<Self, rustorm::row::DecodeError> {
            Ok(Self(row.get("id")?))
        }
    }
    struct TextChild(String);
    impl rustorm::row::FromRow for TextChild {
        fn from_row(row: &rustorm::row::Row) -> Result<Self, rustorm::row::DecodeError> {
            Ok(Self(row.get("parent_id")?))
        }
    }
    let many = rustorm::relations::Many::<TextParent, TextChild, String>::new(
        "r_parent",
        "id",
        "r_child",
        "parent_id",
        &["id", "parent_id"],
    );
    let rows = db
        .include_many(rustorm::query::Select::new("r_parent", &["id"]), many)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].parent.0, "Alpha");
    assert_eq!(rows[0].children.len(), 1);
    assert_eq!(rows[0].children[0].0, "alpha");
    db.execute_raw("INSERT INTO r_child VALUES (2, NULL), (3, 'ALPHA')")
        .unwrap();
    struct NullableTextChild(Option<String>);
    impl rustorm::row::FromRow for NullableTextChild {
        fn from_row(row: &rustorm::row::Row) -> Result<Self, rustorm::row::DecodeError> {
            Ok(Self(row.get("parent_id")?))
        }
    }
    let one = rustorm::relations::One::<NullableTextChild, TextParent, String>::new(
        "r_child",
        "parent_id",
        "r_parent",
        "id",
        true,
        &["id"],
    );
    let select = rustorm::query::Select::new("r_child", &["id", "parent_id"])
        .order_by(rustorm::query::Field::<i32>::new("r_child", "id").asc());
    let rows = db.include_one(select, one).unwrap();
    assert_eq!(
        rows.iter()
            .map(|x| (
                x.child.0.as_deref(),
                x.parent.as_ref().map(|p| p.0.as_str())
            ))
            .collect::<Vec<_>>(),
        vec![
            (Some("alpha"), Some("Alpha")),
            (None, None),
            (Some("ALPHA"), Some("Alpha"))
        ]
    );
    // Exercise more than two batches on a real server, including UNION parameter typing.
    db.execute_raw("CREATE TABLE batch_parent (id integer PRIMARY KEY); CREATE TABLE batch_child (id integer REFERENCES batch_parent(id)); INSERT INTO batch_parent SELECT generate_series(1,1001); INSERT INTO batch_child SELECT generate_series(1,1001)").unwrap();
    struct BatchRecord(i32);
    impl rustorm::row::FromRow for BatchRecord {
        fn from_row(row: &rustorm::row::Row) -> Result<Self, rustorm::row::DecodeError> {
            Ok(Self(row.get("id")?))
        }
    }
    let descriptor = rustorm::relations::One::<BatchRecord, BatchRecord, i32>::new(
        "batch_child",
        "id",
        "batch_parent",
        "id",
        false,
        &["id"],
    );
    let loaded = db
        .include_one(
            rustorm::query::Select::new("batch_child", &["id"]),
            descriptor,
        )
        .unwrap();
    assert_eq!(loaded.len(), 1001);
    assert!(loaded
        .iter()
        .all(|x| x.child.0 == x.parent.as_ref().unwrap().0));
    db.execute_raw("DROP TABLE batch_child; DROP TABLE batch_parent")
        .unwrap();
    db.execute_raw("DROP TABLE r_child; DROP TABLE r_parent; DROP COLLATION rustorm_test_ci")
        .unwrap();
    let mut shadow = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let error = migrations::plan(&mut db, &mut shadow, &dir, &parsed)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not yet support relations"), "{error}");
    let indexed = schema::parse("datasource db {\n provider = \"postgresql\"\n}\nmodel A {\n id Int @id\n @@index([id])\n}\n").unwrap();
    let error = migrations::plan(&mut db, &mut shadow, &dir, &indexed)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("does not yet support custom indexes"),
        "{error}"
    );
    drop(shadow);
    db.execute_raw("DROP TABLE posts; DROP TABLE users; DROP TABLE public._rustorm_migrations")
        .unwrap();
    let optional = SCHEMA
        .replace("authorId Int @map", "authorId Int? @map")
        .replace("author User @relation", "author User? @relation")
        .replace("onDelete: Restrict", "onDelete: SetNull");
    db.execute_raw(&migrations::initial_sql(&schema::parse(&optional).unwrap()).unwrap())
        .unwrap();
    db.execute_raw("INSERT INTO users VALUES (1); INSERT INTO posts VALUES (1,1); DELETE FROM users WHERE id=1").unwrap();
    let rows = db
        .query(&CompiledQuery {
            sql: "SELECT author_id FROM posts WHERE id=1".into(),
            params: vec![],
        })
        .unwrap();
    assert_eq!(rows[0].get::<Option<i32>>("author_id").unwrap(), None);
    drop(db);
    drop(guard);
}
