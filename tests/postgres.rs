#![cfg(feature = "postgres")]
use rustorm::postgres::{Connection, Error};
use rustorm::query::{CompiledQuery, Delete, Field, Insert, QueryError, Select, Update};
use rustorm::row::{DecodeError, FromRow, Row};

const TABLE: &str = "rustorm_integration_test";
const ID: Field<i32> = Field::new(TABLE, "id");
const NAME: Field<String> = Field::new(TABLE, "name");
const ALIAS: Field<Option<String>> = Field::new(TABLE, "alias");
#[derive(Debug, PartialEq)]
struct Record {
    id: i32,
    name: String,
    alias: Option<String>,
}
impl FromRow for Record {
    fn from_row(row: &Row) -> Result<Self, DecodeError> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            alias: row.get("alias")?,
        })
    }
}
fn select() -> Select {
    Select::new(TABLE, &["id", "name", "alias"])
}
fn insert(id: i32, name: &str) -> CompiledQuery {
    Insert::new(TABLE, &["id", "name"])
        .set(ID, id)
        .set(NAME, name)
        .compile()
        .unwrap()
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn real_postgres_crud_transactions_errors_and_tls_policy() {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut db = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    db.execute_raw("DROP TABLE IF EXISTS rustorm_integration_test; CREATE TABLE rustorm_integration_test (id integer PRIMARY KEY, name text NOT NULL UNIQUE, alias text)").unwrap();
    let hostile = "Alice '); DROP TABLE rustorm_integration_test; --";
    let oversized = CompiledQuery {
        sql: "SELECT 1".into(),
        params: vec![rustorm::query::Value::Int(1); rustorm::query::MAX_PARAMETERS + 1],
    };
    assert!(matches!(
        db.query(&oversized),
        Err(Error::InvalidInput("too many parameters"))
    ));
    assert_eq!(db.execute(&insert(1, hostile)).unwrap(), 1);
    let boundary = select()
        .where_(ID.is_in(vec![1; rustorm::query::MAX_PARAMETERS]))
        .compile()
        .unwrap();
    assert_eq!(db.fetch::<Record>(&boundary).unwrap().len(), 1);
    let created = Insert::new(TABLE, &["id", "name"])
        .set(ID, 10)
        .set(NAME, "returning")
        .returning(&["id", "name", "alias"])
        .compile()
        .unwrap();
    assert_eq!(
        db.fetch::<Record>(&created).unwrap(),
        vec![Record {
            id: 10,
            name: "returning".into(),
            alias: None
        }]
    );
    // Executing a RETURNING query through execute must not report an error after changing data.
    let returning_count = Insert::new(TABLE, &["id", "name"])
        .set(ID, 13)
        .set(NAME, "count")
        .returning(&["id", "name", "alias"])
        .compile()
        .unwrap();
    assert_eq!(db.execute(&returning_count).unwrap(), 1);
    let no_returning = Insert::new(TABLE, &["id", "name"])
        .set(ID, 14)
        .set(NAME, "empty rows")
        .compile()
        .unwrap();
    assert!(db.query(&no_returning).unwrap().is_empty());
    let read = select().where_(ID.equals(1)).compile().unwrap();
    assert_eq!(
        db.fetch::<Record>(&read).unwrap(),
        vec![Record {
            id: 1,
            name: hostile.into(),
            alias: None
        }]
    );
    assert_eq!(db.execute(&insert(11, "100%_safe\\ok")).unwrap(), 1);
    assert_eq!(db.execute(&insert(12, "100XXsafeZZok")).unwrap(), 1);
    let literal = select()
        .where_(NAME.contains("%_safe\\"))
        .compile()
        .unwrap();
    let matches = db.fetch::<Record>(&literal).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, 11);
    for filter in [
        NAME.contains_insensitive("%_SAFE\\"),
        NAME.starts_with_insensitive("100%_SAFE"),
        NAME.ends_with_insensitive("_SAFE\\OK"),
    ] {
        let matches = db
            .fetch::<Record>(&select().where_(filter).compile().unwrap())
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, 11);
    }
    let group = rustorm::query::Filter::and(vec![
        ID.gte(10),
        rustorm::query::Filter::or(vec![NAME.starts_with("100%"), ID.equals(10)]),
    ]);
    let matches = db
        .fetch::<Record>(&select().where_(group).compile().unwrap())
        .unwrap();
    assert_eq!(matches.len(), 2);
    let count = rustorm::query::Count::new(TABLE)
        .where_(ID.is_in([11, 12, 999]))
        .compile()
        .unwrap();
    let count = db.fetch::<rustorm::query::CountResult>(&count).unwrap();
    assert_eq!(count, vec![rustorm::query::CountResult { count: 2 }]);
    let selected = db
        .fetch::<Record>(
            &select()
                .where_(ID.is_in([12, 11]))
                .order_by(ID.asc())
                .compile()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        selected.iter().map(|record| record.id).collect::<Vec<_>>(),
        vec![11, 12]
    );
    assert!(db
        .fetch::<Record>(
            &select()
                .where_(ALIAS.not_in_some(["excluded"]))
                .compile()
                .unwrap()
        )
        .unwrap()
        .is_empty());
    let update = Update::new(TABLE)
        .set_some(ALIAS, "Àlice")
        .where_(ID.equals(1))
        .compile()
        .unwrap();
    assert_eq!(db.execute(&update).unwrap(), 1);
    assert_eq!(
        db.fetch::<Record>(&read).unwrap()[0].alias.as_deref(),
        Some("Àlice")
    );
    let selected = db
        .fetch::<Record>(
            &select()
                .where_(ALIAS.in_some(["Àlice", "excluded"]))
                .where_(ALIAS.contains_some("Àl"))
                .compile()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, 1);
    let selected = db
        .fetch::<Record>(
            &select()
                .where_(ALIAS.not_in_some(["excluded"]))
                .compile()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, 1);
    db.execute_raw("UPDATE rustorm_integration_test SET alias = 'same' WHERE id IN (11, 12)")
        .unwrap();
    let ordered = db
        .fetch::<Record>(
            &select()
                .where_(ID.is_in([1, 10, 11]))
                .order_by(ALIAS.asc_nulls_first())
                .compile()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(ordered[0].id, 10);
    let ordered = db
        .fetch::<Record>(
            &select()
                .where_(ID.is_in([1, 10, 11]))
                .order_by(ALIAS.desc_nulls_last())
                .compile()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(ordered[2].id, 10);
    let q = rustorm::query::Count::new(TABLE)
        .field(ALIAS)
        .compile()
        .unwrap();
    assert_eq!(
        db.fetch::<rustorm::query::CountResult>(&q).unwrap()[0].count,
        3
    );
    let q = rustorm::query::Count::new(TABLE)
        .field(ALIAS)
        .distinct()
        .compile()
        .unwrap();
    assert_eq!(
        db.fetch::<rustorm::query::CountResult>(&q).unwrap()[0].count,
        2
    );
    let q = rustorm::query::Count::new(TABLE)
        .field(ALIAS)
        .distinct()
        .where_(ID.is_in([11, 12]))
        .compile()
        .unwrap();
    assert_eq!(
        db.fetch::<rustorm::query::CountResult>(&q).unwrap()[0].count,
        1
    );
    assert_eq!(
        Update::new(TABLE).set_null(ALIAS).compile().unwrap_err(),
        QueryError::UnfilteredMutation
    );
    let err = db.execute(&insert(2, hostile)).unwrap_err();
    assert!(matches!(&err, Error::UniqueViolation { .. }) || matches!(&err, Error::Server { code } if code.as_deref() == Some("23505")), "got {:?}", err);
    assert!(!format!("{err:?}").contains(hostile));
    // An error in a transaction must not be reported as a successful commit.
    {
        let mut tx = db.begin().unwrap();
        tx.execute(&insert(3, "rolled back")).unwrap();
        assert!(tx.execute(&insert(4, hostile)).is_err());
        assert!(matches!(tx.commit(), Err(Error::TransactionAborted)));
    }
    assert!(db
        .fetch::<Record>(&select().where_(ID.equals(3)).compile().unwrap())
        .unwrap()
        .is_empty());
    {
        let mut tx = db.begin().unwrap();
        tx.execute(&insert(5, "abandoned")).unwrap();
    }
    assert!(db
        .fetch::<Record>(&select().where_(ID.equals(5)).compile().unwrap())
        .unwrap()
        .is_empty());
    {
        let mut tx = db.begin().unwrap();
        tx.execute(&insert(6, "committed")).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(
        db.fetch::<Record>(&select().where_(ID.equals(6)).compile().unwrap())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.execute(&Delete::new(TABLE).where_(ID.equals(1)).compile().unwrap())
            .unwrap(),
        1
    );
    assert_eq!(db.fetch::<Record>(&read).unwrap().len(), 0);
    db.execute_raw("DROP TABLE rustorm_integration_test")
        .unwrap();
    let conninfo = format!("host=127.0.0.1 port={port} user={user} dbname={database} sslmode=disable connect_timeout=3");
    assert!(
        Connection::connect(&conninfo).is_err(),
        "secure connector must not honor sslmode=disable"
    );
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn generated_model_runs_end_to_end_on_postgres() {
    use std::fs;
    use std::process::Command;
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let temp =
        Temp(std::env::temp_dir().join(format!("rustorm-postgres-client-{}", std::process::id())));
    fs::create_dir_all(&temp.0).unwrap();
    let cli = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rustorm"))
            .current_dir(&temp.0)
            .args(args)
            .output()
            .unwrap()
    };
    let init = cli(&["init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let schema_path = temp.0.join("rustorm/schema.rsm");
    let schema = fs::read_to_string(&schema_path).unwrap().replace(
        "@@map(\"users\")",
        "@@map(\"rustorm_generated_integration\")",
    );
    fs::write(schema_path, schema).unwrap();
    let generated = cli(&["generate"]);
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(temp.0.join("Cargo.toml"), format!("[package]\nname=\"generated_postgres_test\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(temp.0.join("src/main.rs"), r#"
mod generated { pub mod rustorm; }
use generated::rustorm::user;
use rustorm::postgres::Connection;
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
 let name = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
 let database = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
 let mut db = Connection::connect_local_unencrypted(port, &name, &database)?;
 db.execute_raw("DROP TABLE IF EXISTS rustorm_generated_integration; CREATE TABLE rustorm_generated_integration (id integer GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY, email text NOT NULL UNIQUE, name text, published boolean NOT NULL DEFAULT false)")?;
 let q = user::create().set(user::EMAIL, "eve@example.org").set_some(user::NAME, "Eve").returning(user::columns()).compile()?;
 let inserted = db.fetch::<user::Record>(&q)?;
 assert_eq!(inserted.len(), 1); assert_eq!(inserted[0].name.as_deref(), Some("Eve"));
 let q = user::find_many().where_(user::EMAIL.equals("eve@example.org")).compile()?;
 let found = db.fetch::<user::Record>(&q)?;
 assert_eq!(found, inserted);
 let q = user::count().where_(user::NAME.in_some(["Eve", "Ada"])).compile()?;
 assert_eq!(db.fetch::<rustorm::query::CountResult>(&q)?[0].count, 1);
 let q = user::count().where_(user::ID.is_in([found[0].id + 100])).compile()?;
 assert_eq!(db.fetch::<rustorm::query::CountResult>(&q)?[0].count, 0);
 let q = user::find_many().select(user::EMAIL).distinct().where_(user::EMAIL.contains_insensitive("EVE@")).compile()?;
 assert_eq!(db.query(&q)?[0].get::<String>("email")?, "eve@example.org");
 let q = user::update_many().increment(user::ID, 10).where_(user::ID.equals(found[0].id)).returning(user::columns()).compile()?;
 let updated = db.fetch::<user::Record>(&q)?;
 assert_eq!(updated[0].id, found[0].id + 10);
 let q = user::delete_many().where_(user::ID.equals(updated[0].id)).returning(user::columns()).compile()?;
 assert_eq!(db.fetch::<user::Record>(&q)?.len(), 1);
 db.execute_raw("DROP TABLE rustorm_generated_integration")?;
 Ok(())
}
"#).unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(&temp.0)
        .args(["run", "--offline", "--quiet"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires a disposable local PostgreSQL server with trust authentication"]
fn atomic_updates_projection_and_case_insensitive_search_against_postgres() {
    const T: &str = "rustorm_query_features";
    const KEY: Field<i32> = Field::new(T, "id");
    const AMOUNT: Field<i32> = Field::new(T, "amount");
    const BIG: Field<i64> = Field::new(T, "big");
    const RATIO: Field<f64> = Field::new(T, "ratio");
    const OPTIONAL: Field<Option<i32>> = Field::new(T, "optional");
    const LABEL: Field<Option<String>> = Field::new(T, "label");
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut db = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    db.execute_raw("DROP TABLE IF EXISTS rustorm_query_features; CREATE TABLE rustorm_query_features (id int PRIMARY KEY, amount int NOT NULL, big bigint NOT NULL, ratio double precision NOT NULL, optional int, label text)").unwrap();
    db.execute_raw("INSERT INTO rustorm_query_features VALUES (1,10,20,3,NULL,'Alpha%_\\X'), (2,10,20,3,8,'alpha2'), (3,10,20,3,NULL,'alpha2'), (4,10,20,3,NULL,NULL)").unwrap();
    let q = Update::new(T)
        .increment(AMOUNT, 5)
        .decrement(BIG, 3_i64)
        .multiply(RATIO, 2.0)
        .increment_some(OPTIONAL, 2)
        .where_(KEY.equals(1))
        .returning(&["amount", "big", "ratio", "optional"])
        .compile()
        .unwrap();
    let rows = db.query(&q).unwrap();
    assert_eq!(rows[0].get::<i32>("amount").unwrap(), 15);
    assert_eq!(rows[0].get::<i64>("big").unwrap(), 17);
    assert_eq!(rows[0].get::<f64>("ratio").unwrap(), 6.0);
    assert_eq!(rows[0].get::<Option<i32>>("optional").unwrap(), None);
    let q = Update::new(T)
        .divide(RATIO, 2.0)
        .where_(KEY.equals(1))
        .returning(&["ratio"])
        .compile()
        .unwrap();
    assert_eq!(db.query(&q).unwrap()[0].get::<f64>("ratio").unwrap(), 3.0);
    let q = Update::new(T)
        .divide(AMOUNT, 2)
        .where_(KEY.equals(1))
        .returning(&["amount"])
        .compile()
        .unwrap();
    assert_eq!(db.query(&q).unwrap()[0].get::<i32>("amount").unwrap(), 7);
    let q = Update::new(T)
        .decrement(AMOUNT, 2)
        .where_(KEY.equals(1))
        .compile()
        .unwrap();
    assert_eq!(db.execute(&q).unwrap(), 1);
    let q = Update::new(T)
        .multiply(AMOUNT, 3)
        .where_(KEY.equals(1))
        .compile()
        .unwrap();
    assert_eq!(db.execute(&q).unwrap(), 1);
    for q in [
        Update::new(T)
            .divide_some(OPTIONAL, 2)
            .where_(KEY.equals(2))
            .compile()
            .unwrap(),
        Update::new(T)
            .increment_some(OPTIONAL, 1)
            .where_(KEY.equals(2))
            .compile()
            .unwrap(),
        Update::new(T)
            .decrement_some(OPTIONAL, 2)
            .where_(KEY.equals(2))
            .compile()
            .unwrap(),
        Update::new(T)
            .multiply_some(OPTIONAL, 3)
            .where_(KEY.equals(2))
            .compile()
            .unwrap(),
    ] {
        assert_eq!(db.execute(&q).unwrap(), 1);
    }
    let q = Select::new(T, &["optional"])
        .where_(KEY.equals(2))
        .compile()
        .unwrap();
    assert_eq!(
        db.query(&q).unwrap()[0]
            .get::<Option<i32>>("optional")
            .unwrap(),
        Some(9)
    );
    let q = Select::new(T, &["id"])
        .where_(LABEL.contains_insensitive_some("%_\\x"))
        .compile()
        .unwrap();
    assert_eq!(db.query(&q).unwrap()[0].get::<i32>("id").unwrap(), 1);
    let q = Select::new(T, &["id"])
        .where_(LABEL.starts_with_insensitive_some("ALPHA"))
        .compile()
        .unwrap();
    assert_eq!(db.query(&q).unwrap().len(), 3);
    let q = Select::new(T, &["id"])
        .where_(LABEL.ends_with_insensitive_some("A2"))
        .compile()
        .unwrap();
    assert_eq!(db.query(&q).unwrap().len(), 2);
    let q = Select::new(T, &["id", "label"])
        .select(LABEL)
        .distinct()
        .order_by(LABEL.asc_nulls_last())
        .compile()
        .unwrap();
    let rows = db.query(&q).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2].get::<Option<String>>("label").unwrap(), None);
    assert!(rows[0].get::<i32>("id").is_err());
    // Multiple independent connections update one row without a read-modify-write race.
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let user = user.clone();
            let database = database.clone();
            std::thread::spawn(move || {
                let mut c = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
                let q = Update::new(T)
                    .increment(AMOUNT, 1)
                    .where_(KEY.equals(1))
                    .compile()
                    .unwrap();
                for _ in 0..40 {
                    assert_eq!(c.execute(&q).unwrap(), 1);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let q = Select::new(T, &["amount"])
        .where_(KEY.equals(1))
        .compile()
        .unwrap();
    assert_eq!(db.query(&q).unwrap()[0].get::<i32>("amount").unwrap(), 175);
    // A server-side integer overflow leaves all assignments in the statement unchanged.
    db.execute_raw("UPDATE rustorm_query_features SET amount=2147483647 WHERE id=1")
        .unwrap();
    let q = Update::new(T)
        .increment(AMOUNT, 1)
        .decrement(BIG, 1_i64)
        .where_(KEY.equals(1))
        .compile()
        .unwrap();
    assert!(
        matches!(db.execute(&q), Err(Error::Server { code }) if code.as_deref() == Some("22003"))
    );
    let q = Select::new(T, &["amount", "big"])
        .where_(KEY.equals(1))
        .compile()
        .unwrap();
    let rows = db.query(&q).unwrap();
    assert_eq!(rows[0].get::<i32>("amount").unwrap(), i32::MAX);
    assert_eq!(rows[0].get::<i64>("big").unwrap(), 17);
    db.execute_raw("DROP TABLE rustorm_query_features").unwrap();
}
