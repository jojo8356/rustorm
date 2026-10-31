#![cfg(feature = "postgres")]
use rustorm::{migrations, postgres::Connection, schema};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model Parent {
 id Int @id
 children Child[]
 compositeChildren CompositeChild[]
 badge Badge?
 requiredChildren RequiredChild[]
 requiredBadge RequiredBadge?
 @@map("parents")
}
model Child {
 id Int @id
 parentId Int? @map("parent_id")
 parent Parent? @relation(fields: [parentId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 @@map("children")
}
model CompositeChild {
 id Int
 language String
 parentId Int? @map("parent_id")
 parent Parent? @relation(fields: [parentId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 @@id([id, language])
 @@map("composite_children")
}
model Badge {
 id Int @id
 parentId Int? @unique @map("parent_id")
 parent Parent? @relation(fields: [parentId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 @@map("badges")
}
model RequiredBadge {
 id Int @id
 parentId Int @unique @map("parent_id")
 parent Parent @relation(fields: [parentId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 @@map("required_badges")
}
model RequiredChild {
 id Int @id
 parentId Int @map("parent_id")
 parent Parent @relation(fields: [parentId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 @@map("required_children")
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
fn disconnect_preserves_entities_and_respects_transactions_and_cardinality() {
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let database =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &database).unwrap();
    let name = format!("rustorm_disconnect_{}", std::process::id());
    admin
        .execute_raw(&format!("CREATE DATABASE \"{name}\""))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-disconnect-{}", std::process::id()));
    let guard = Cleanup {
        admin,
        name: name.clone(),
        dir: dir.clone(),
    };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    migrations::deploy(&mut db, &dir).unwrap();
    db.execute_raw("INSERT INTO parents VALUES (1),(2); INSERT INTO children VALUES (11,1),(12,1),(13,2),(14,NULL); INSERT INTO badges VALUES (21,1),(22,2); INSERT INTO required_children VALUES (31,1); INSERT INTO required_badges VALUES (41,1); INSERT INTO composite_children VALUES (50,'fr',1),(50,'en',1)").unwrap();
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"),format!("[package]\nname=\"disconnect_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"),r#"mod generated;
use generated::{parent,child,badge,required_child,required_badge,composite_child};
use rustorm::{relations::{NestedChange,NestedSet},postgres::{Connection,Error},query::QueryError};
fn main() {
 let port=std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap().parse().unwrap();
 let user=std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_|"rustorm".into());
 let dbname=std::env::var("RUSTORM_DISCONNECT_DB").unwrap();
 let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();
 // Refuse a transactional trigger or rewrite rule rather than report a false disconnect.
 db.execute_raw("CREATE FUNCTION keep_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.parent_id := OLD.parent_id; RETURN NEW; END $$; CREATE TRIGGER keep_link BEFORE UPDATE ON children FOR EACH ROW EXECUTE FUNCTION keep_link()").unwrap();
 let mut tx=db.begin().unwrap();
 let guarded=NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap();
 assert!(matches!(tx.nested_change(guarded),Err(Error::InvalidInput(_))));
 assert!(matches!(tx.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![11]).unwrap()),Err(Error::InvalidInput(_))));
 let q=child::find_many().where_(child::ID.equals(11)).compile().unwrap();
 assert_eq!(tx.fetch::<child::Record>(&q).unwrap()[0].parent_id,Some(1));
 tx.rollback().unwrap();
 // Disabled user triggers do not fire and may be safely ignored.
 db.execute_raw("ALTER TABLE children DISABLE TRIGGER keep_link").unwrap();
 let mut tx=db.begin().unwrap();
 tx.nested_change(NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap()).unwrap();
 tx.rollback().unwrap();
 db.execute_raw("DROP TRIGGER keep_link ON children; DROP FUNCTION keep_link(); CREATE FUNCTION reattach() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE children SET parent_id = OLD.parent_id WHERE id = NEW.id; RETURN NULL; END $$; CREATE TRIGGER reattach AFTER UPDATE ON children FOR EACH ROW WHEN (OLD.parent_id IS NOT NULL AND NEW.parent_id IS NULL) EXECUTE FUNCTION reattach()").unwrap();
 assert!(matches!(db.nested_change(NestedChange::disconnect_owner(child::parent(),1,child::ID.equals(11)).unwrap()),Err(Error::InvalidInput(_))));
 db.execute_raw("DROP TRIGGER reattach ON children; DROP FUNCTION reattach(); CREATE RULE keep_link AS ON UPDATE TO children DO INSTEAD NOTHING").unwrap();
 assert!(matches!(db.nested_change(NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap()),Err(Error::InvalidInput(_))));
 assert!(matches!(db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![11]).unwrap()),Err(Error::InvalidInput(_))));
 db.execute_raw("DROP RULE keep_link ON children").unwrap();
 let mut tx=db.begin().unwrap();
 tx.execute_statement("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").unwrap();
 assert!(matches!(tx.nested_change(NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap()),Err(Error::InvalidInput(_))));
 assert!(matches!(tx.nested_set(NestedSet::clear_many(parent::children(),1,child::ID).unwrap()),Err(Error::InvalidInput(_))));
 tx.rollback().unwrap();
 // DDL cannot install a new trigger after inspection and before commit.
 db.execute_raw("CREATE FUNCTION keep_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.parent_id := OLD.parent_id; RETURN NEW; END $$").unwrap();
 let mut tx=db.begin().unwrap();
 tx.nested_change(NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap()).unwrap();
 let mut other=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();
 other.execute_raw("SET lock_timeout = '100ms'").unwrap();
 assert!(other.execute_raw("CREATE TRIGGER keep_link BEFORE UPDATE ON children FOR EACH ROW EXECUTE FUNCTION keep_link()").is_err());
 tx.rollback().unwrap();
 db.execute_raw("DROP FUNCTION keep_link()").unwrap();
 // Missing FK or external dependencies on the mutable FK are rejected.
 let mut tx=db.begin().unwrap();
 let fk=tx.query(&rustorm::query::CompiledQuery {sql:"SELECT conname::text AS conname FROM pg_constraint WHERE conrelid='children'::regclass AND contype='f'".into(),params:vec![]}).unwrap()[0].get::<String>("conname").unwrap();
 tx.execute_statement(&format!("ALTER TABLE children DROP CONSTRAINT \"{}\"",fk.replace('\"',"\"\""))).unwrap();
 assert!(matches!(tx.nested_set(NestedSet::clear_many(parent::children(),1,child::ID).unwrap()),Err(Error::InvalidInput(_))));
 tx.rollback().unwrap();
 let mut tx=db.begin().unwrap();
 tx.execute_statement("UPDATE children SET parent_id=NULL WHERE id=12").unwrap();
 tx.execute_statement("CREATE UNIQUE INDEX temp_parent_unique ON children(parent_id)").unwrap();
 tx.execute_statement("CREATE TABLE temp_parent_ref (id int REFERENCES children(parent_id) ON UPDATE CASCADE)").unwrap();
 assert!(matches!(tx.nested_set(NestedSet::clear_many(parent::children(),1,child::ID).unwrap()),Err(Error::InvalidInput(_))));
 tx.rollback().unwrap();
 // Replace a 1–N collection without reassigning another parent's child.
 assert!(matches!(NestedSet::many(parent::required_children(),1,required_child::ID,vec![31]),Err(QueryError::RequiredRelationViolation)));
 let mut tx=db.begin().unwrap();
 let conflict=NestedSet::many(parent::children(),1,child::ID,vec![12,13]).unwrap();
 assert!(matches!(tx.nested_set(conflict),Err(Error::RelationConflict)));
 tx.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![12,14]).unwrap()).unwrap();
 let q=child::find_many().where_(child::ID.equals(11)).compile().unwrap();
 assert_eq!(tx.fetch::<child::Record>(&q).unwrap()[0].parent_id,None);
 tx.rollback().unwrap();
 assert_eq!(db.fetch::<child::Record>(&q).unwrap()[0].parent_id,Some(1));
 assert!(matches!(db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![12,999]).unwrap()),Err(Error::RelationNotFound)));
 db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![12,14]).unwrap()).unwrap();
 assert_eq!(db.fetch::<child::Record>(&q).unwrap()[0].parent_id,None);
 let q13=child::find_many().where_(child::ID.equals(13)).compile().unwrap();
 assert_eq!(db.fetch::<child::Record>(&q13).unwrap()[0].parent_id,Some(2));
 db.nested_set(NestedSet::clear_many(parent::children(),1,child::ID).unwrap()).unwrap();
 let q12=child::find_many().where_(child::ID.equals(12)).compile().unwrap();
 assert_eq!(db.fetch::<child::Record>(&q12).unwrap()[0].parent_id,None);
 db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![11,12]).unwrap()).unwrap();
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let threads=[11,12].into_iter().map(|id| {let barrier=barrier.clone();let user=user.clone();let dbname=dbname.clone();
  std::thread::spawn(move || {let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();barrier.wait();db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![id]).unwrap())}) }).collect::<Vec<_>>();
 assert!(threads.into_iter().all(|t|t.join().unwrap().is_ok()));
 let linked=db.include_many(parent::find_many().where_(parent::ID.equals(1)),parent::children()).unwrap();
 assert_eq!(linked[0].children.len(),1);
 db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![11,12]).unwrap()).unwrap();
 let required=NestedChange::disconnect(parent::required_children(),1,required_child::ID.equals(31));
 assert!(matches!(required,Err(QueryError::RequiredRelationViolation)));
 assert!(matches!(NestedChange::disconnect_owner(required_child::parent(),1,required_child::ID.equals(31)),Err(QueryError::RequiredRelationViolation)));
 assert!(matches!(NestedChange::disconnect_one(parent::required_badge(),1,required_badge::ID.equals(41)),Err(QueryError::RequiredRelationViolation)));
 assert!(matches!(NestedChange::disconnect_owner(required_badge::parent(),1,required_badge::ID.equals(41)),Err(QueryError::RequiredRelationViolation)));
 assert!(matches!(NestedChange::disconnect(parent::composite_children(),1,composite_child::ID.equals(50)),Err(QueryError::NonUniqueNestedSelector)));
 let compound=rustorm::query::Filter::and(vec![composite_child::ID.equals(50), composite_child::LANGUAGE.equals("fr")]);
 db.nested_change(NestedChange::disconnect(parent::composite_children(),1,compound).unwrap()).unwrap();
 let cq=composite_child::find_many().compile().unwrap();
 let rows=db.fetch::<composite_child::Record>(&cq).unwrap();
 assert_eq!(rows.iter().find(|c| c.language=="fr").unwrap().parent_id,None);
 assert_eq!(rows.iter().find(|c| c.language=="en").unwrap().parent_id,Some(1));
 let wrong=NestedChange::disconnect(parent::children(),2,child::ID.equals(11)).unwrap();
 assert!(matches!(db.nested_change(wrong),Err(Error::RelationNotFound)));
 assert!(matches!(NestedChange::disconnect(parent::children(),1,child::ID.gte(11)),Err(QueryError::NonUniqueNestedSelector)));
 assert!(matches!(NestedChange::disconnect_one(parent::badge(),1,badge::PARENT_ID.is_null()),Err(QueryError::NonUniqueNestedSelector)));
 let q=child::find_many().where_(child::ID.equals(11)).compile().unwrap();
 assert_eq!(db.fetch::<child::Record>(&q).unwrap()[0].parent_id,Some(1));
 let mut tx=db.begin().unwrap();
 let missing=NestedChange::disconnect(parent::children(),1,child::ID.equals(99)).unwrap();
 assert!(matches!(tx.nested_change(missing),Err(Error::RelationNotFound)));
 let q=child::find_many().where_(child::ID.equals(11)).compile().unwrap();
 assert_eq!(tx.fetch::<child::Record>(&q).unwrap()[0].parent_id,Some(1));
 tx.nested_change(NestedChange::disconnect_one(parent::badge(),1,badge::ID.equals(21)).unwrap()).unwrap();
 let qbad=badge::find_many().where_(badge::ID.equals(21)).compile().unwrap();
 assert_eq!(tx.fetch::<badge::Record>(&qbad).unwrap()[0].parent_id,None);
 tx.rollback().unwrap();
 assert_eq!(db.fetch::<badge::Record>(&qbad).unwrap()[0].parent_id,Some(1));
 db.nested_change(NestedChange::disconnect_one(parent::badge(),1,badge::ID.equals(21)).unwrap()).unwrap();
 assert_eq!(db.fetch::<badge::Record>(&qbad).unwrap()[0].parent_id,None);
 db.nested_change(NestedChange::disconnect_owner(badge::parent(),2,badge::PARENT_ID.equals_some(2)).unwrap()).unwrap();
 let q=badge::find_many().where_(badge::ID.equals(22)).compile().unwrap();
 assert_eq!(db.fetch::<badge::Record>(&q).unwrap()[0].parent_id,None);
 db.nested_change(NestedChange::disconnect_owner(child::parent(),2,child::ID.equals(13)).unwrap()).unwrap();
 let q=child::find_many().where_(child::ID.equals(13)).compile().unwrap();
 assert_eq!(db.fetch::<child::Record>(&q).unwrap()[0].parent_id,None);
 let retry=NestedChange::disconnect_owner(child::parent(),2,child::ID.equals(13)).unwrap();
 assert!(matches!(db.nested_change(retry),Err(Error::RelationNotFound)));
 db.nested_change(NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap()).unwrap();
 let q=child::find_many().where_(child::ID.equals(11)).compile().unwrap();
 assert_eq!(db.fetch::<child::Record>(&q).unwrap()[0].parent_id,None);
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let threads=(0..2).map(|_| { let barrier=barrier.clone();let user=user.clone();let dbname=dbname.clone(); std::thread::spawn(move || {
  let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();barrier.wait();
  db.nested_change(NestedChange::disconnect(parent::children(),1,child::ID.equals(12)).unwrap())
 }) }).collect::<Vec<_>>();
 let outcomes=threads.into_iter().map(|h|h.join().unwrap()).collect::<Vec<_>>();
 assert_eq!(outcomes.iter().filter(|r|r.is_ok()).count(),1);
 assert_eq!(outcomes.iter().filter(|r|matches!(r,Err(Error::RelationNotFound))).count(),1);
 assert_eq!(db.fetch::<child::Record>(&child::find_many().compile().unwrap()).unwrap().len(),4);
 assert_eq!(db.fetch::<parent::Record>(&parent::find_many().compile().unwrap()).unwrap().len(),2);
 assert_eq!(db.fetch::<badge::Record>(&badge::find_many().compile().unwrap()).unwrap().len(),2);
 // Simulate schema drift: when the promised unique constraint is missing, rollback all affected rows.
 db.execute_raw("ALTER TABLE children DROP CONSTRAINT children_pkey; UPDATE children SET parent_id=1 WHERE id=11; INSERT INTO children VALUES (11,1)").unwrap();
 assert!(matches!(db.nested_set(NestedSet::many(parent::children(),1,child::ID,vec![11]).unwrap()),Err(Error::InvalidInput(_))));
 let mut tx=db.begin().unwrap();
 let plan=NestedChange::disconnect(parent::children(),1,child::ID.equals(11)).unwrap();
 assert!(matches!(tx.nested_change(plan),Err(Error::RelationAmbiguous)));
 let q=child::find_many().where_(child::ID.equals(11)).compile().unwrap();
 let rows=tx.fetch::<child::Record>(&q).unwrap();
 assert_eq!(rows.len(),2); assert!(rows.iter().all(|c|c.parent_id==Some(1)));
 // Savepoint rollback left the outer transaction usable.
 tx.rollback().unwrap();
}
"#).unwrap();
    let out =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&consumer)
            .args(["run", "--offline", "--quiet"])
            .env("RUSTORM_DISCONNECT_DB", &name)
            .output()
            .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    drop(db);
    drop(guard);
}
