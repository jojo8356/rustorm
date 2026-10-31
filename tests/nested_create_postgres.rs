#![cfg(feature = "postgres")]
use rustorm::{migrations, postgres::Connection, schema};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model User {
 id Int @id @default(autoincrement())
 posts Post[]
 profile Profile?
 tags Tag[]
 @@map("users")
}
model Post {
 id Int @id
 title String @unique
 note String @default("")
 authorId Int @map("author_id")
 author User @relation(fields: [authorId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 @@map("posts")
}
model Tag {
 id String @id
 name String @default("")
 users User[]
 @@map("tags")
}
model Profile {
 id Int @id
 bio String @default("")
 userId Int @unique @map("user_id")
 user User @relation(fields: [userId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 @@map("profiles")
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
        let _ = self
            .admin
            .execute_raw(&format!("DROP ROLE IF EXISTS \"{}_rls\"", self.name));
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[test]
#[ignore = "requires disposable PostgreSQL and CREATEDB privilege"]
fn nested_creation_is_atomic_and_respects_outer_transactions() {
    let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let admin_db =
        std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &admin_db).unwrap();
    let name = format!("rustorm_nested_{}", std::process::id());
    admin
        .execute_raw(&format!("CREATE DATABASE \"{name}\""))
        .unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-nested-{}", std::process::id()));
    let guard = Cleanup {
        admin,
        name: name.clone(),
        dir: dir.clone(),
    };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    migrations::deploy(&mut db, &dir).unwrap();
    db.execute_raw("INSERT INTO tags (id) VALUES ('rust'),('db')")
        .unwrap();
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"),format!("[package]\nname=\"nested_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"),r#"mod generated;
use generated::{user,post,profile,tag};
use rustorm::{relations::{NestedCreate,NestedChange,NestedSet}, postgres::{Connection,Error}};
fn plan(id:i32, title:&str) -> NestedCreate<user::Record,post::Record,i32> {
 NestedCreate::many(user::create(),user::posts(),vec![post::create().set(post::ID,id).set(post::TITLE,title)]).unwrap()
}
fn count(db:&mut Connection) -> (i64,i64) {
 (db.fetch::<rustorm::query::CountResult>(&user::count().compile().unwrap()).unwrap()[0].count,
 db.fetch::<rustorm::query::CountResult>(&post::count().compile().unwrap()).unwrap()[0].count)
}
fn main() {
 let port: u16 = std::env::var("RUSTORM_TEST_DATABASE_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(5432);
 let user=std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_|"rustorm".into());
 let dbname=std::env::var("RUSTORM_NESTED_DB").unwrap();
 let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();
 let request=NestedCreate::many(user::create(),user::posts(),vec![post::create().set(post::ID,1).set(post::TITLE,"one"),post::create().set(post::ID,2).set(post::TITLE,"two")]).unwrap();
 let id=db.nested_create(request).unwrap();
 let first_parent=id;
 let users=db.include_many(user::find_many(),user::posts()).unwrap();
 assert_eq!(users.len(),1); assert_eq!(users[0].parent.id,id); assert_eq!(users[0].children.len(),2); assert!(users[0].children.iter().all(|c|c.author_id==id));
 let one=NestedCreate::one(user::create(),user::profile(),profile::create().set(profile::ID,11)).unwrap();
 let id2=db.nested_create(one).unwrap();
 let q=profile::find_many().where_(profile::ID.equals(11)).compile().unwrap();
 assert_eq!(db.fetch::<profile::Record>(&q).unwrap()[0].user_id,id2);
 db.nested_change(NestedChange::update_one(user::profile(),id2,profile::ID.equals(11),profile::update_many().set(profile::BIO,"hello")).unwrap()).unwrap();
 assert_eq!(db.fetch::<profile::Record>(&q).unwrap()[0].bio,"hello");
 let wrong=NestedChange::delete_one(user::profile(),first_parent,profile::ID.equals(11),profile::delete_many()).unwrap();
 assert!(matches!(db.nested_change(wrong),Err(Error::RelationNotFound)));
 let before=count(&mut db);
 // Connection entry point must not take over a transaction started with raw SQL.
 db.execute_raw("BEGIN").unwrap();
 assert!(matches!(db.nested_create(plan(50,"not committed")),Err(Error::InvalidInput(_))));
 db.execute_raw("ROLLBACK").unwrap();
 assert_eq!(count(&mut db),before);
 // Ambiguous operations roll back to their savepoint and keep the outer transaction usable.
 let mut tx=db.begin().unwrap();
 let broad=NestedChange::delete(user::posts(),first_parent,post::ID.gte(1),post::delete_many()).unwrap();
 assert!(matches!(tx.nested_change(broad),Err(Error::RelationAmbiguous)));
 tx.nested_create(plan(51,"after ambiguous delete")).unwrap();
 tx.rollback().unwrap();
 assert_eq!(count(&mut db),before);
 // The scoped predicate must not touch another parent's child.
 let wrong=NestedChange::update(user::posts(),id2,post::ID.equals(1),post::update_many().set(post::TITLE,"wrong")).unwrap();
 assert!(matches!(db.nested_change(wrong),Err(Error::RelationNotFound)));
 // Non-unique filters roll back all candidate changes.
 let broad=NestedChange::update(user::posts(),first_parent,post::ID.gte(1),post::update_many().set(post::NOTE,"bad")).unwrap();
 assert!(matches!(db.nested_change(broad),Err(Error::RelationAmbiguous)));
 let untouched=post::find_many().where_(post::ID.equals(1)).compile().unwrap();
 assert_eq!(db.fetch::<post::Record>(&untouched).unwrap()[0].note,"");
 let broad=NestedChange::delete(user::posts(),first_parent,post::ID.gte(1),post::delete_many()).unwrap();
 assert!(matches!(db.nested_change(broad),Err(Error::RelationAmbiguous)));
 assert_eq!(count(&mut db),before);
 let update=NestedChange::update(user::posts(),first_parent,post::ID.equals(1),post::update_many().set(post::TITLE,"changed")).unwrap();
 db.nested_change(update).unwrap();
 let q=post::find_many().where_(post::ID.equals(1)).compile().unwrap();
 assert_eq!(db.fetch::<post::Record>(&q).unwrap()[0].title,"changed");
 db.nested_change(NestedChange::update(user::posts(),first_parent,post::ID.equals(1),post::update_many().set(post::TITLE,"one")).unwrap()).unwrap();
 let fail=NestedCreate::many(user::create(),user::posts(),vec![post::create().set(post::ID,3).set(post::TITLE,"fresh"),post::create().set(post::ID,4).set(post::TITLE,"one")]).unwrap();
 assert!(db.nested_create(fail).is_err()); assert_eq!(count(&mut db),before);
 let mut tx=db.begin().unwrap();
 let key=tx.nested_create(plan(5,"in transaction")).unwrap();
 let q=user::find_many().where_(user::ID.equals(key)).compile().unwrap();
 assert_eq!(tx.fetch::<user::Record>(&q).unwrap().len(),1);
 tx.rollback().unwrap(); assert_eq!(count(&mut db),before);
 let mut tx=db.begin().unwrap();
 let preserved=tx.nested_create(plan(6,"preserved")).unwrap();
 assert!(tx.nested_create(plan(7,"one")).is_err());
 let later=tx.nested_create(plan(8,"after error")).unwrap();
 assert_ne!(preserved,later); tx.commit().unwrap();
 assert_eq!(count(&mut db),(before.0+2,before.1+2));
 // Two connections race to create different parents with the same unique child value.
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let handles=(0..2).map(|i| { let barrier=barrier.clone(); let user=user.clone(); let dbname=dbname.clone();
 std::thread::spawn(move || {let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap(); barrier.wait(); db.nested_create(plan(100+i,"raced")).is_ok()}) }).collect::<Vec<_>>();
 assert_eq!(handles.into_iter().map(|h|h.join().unwrap()).filter(|ok|*ok).count(),1);
 assert_eq!(count(&mut db),(before.0+3,before.1+3));
 let baseline=count(&mut db);
 let request=NestedCreate::connect(user::create(),user::tags(),vec!["rust".to_owned(),"db".to_owned()]).unwrap();
 let id=db.nested_create(request).unwrap();
 let linked=db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap();
 assert_eq!(linked.len(),1); assert_eq!(linked[0].children.len(),2);
 // Collection replacement validates all targets before touching existing links.
 let bad=NestedSet::through(user::tags(),id,vec!["db".to_owned(),"missing".to_owned()]).unwrap();
 assert!(matches!(db.nested_set(bad),Err(Error::RelationNotFound)));
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),2);
 let mut tx=db.begin().unwrap();
 let missing=NestedSet::through(user::tags(),id,vec!["db".to_owned(),"missing".to_owned()]).unwrap();
 assert!(matches!(tx.nested_set(missing),Err(Error::RelationNotFound)));
 tx.nested_set(NestedSet::through(user::tags(),id,vec!["db".to_owned()]).unwrap()).unwrap();
 assert_eq!(tx.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),1);
 tx.rollback().unwrap();
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),2);
 db.nested_set(NestedSet::clear_through(user::tags(),id).unwrap()).unwrap();
 assert!(db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.is_empty());
 db.nested_set(NestedSet::through(user::tags(),id,vec!["rust".to_owned(),"db".to_owned()]).unwrap()).unwrap();
 assert!(matches!(db.nested_set(NestedSet::clear_through(user::tags(),i32::MAX).unwrap()),Err(Error::RelationNotFound)));
 let mut tx=db.begin().unwrap();
 tx.execute_statement("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").unwrap();
 assert!(matches!(tx.nested_set(NestedSet::clear_through(user::tags(),id).unwrap()),Err(Error::InvalidInput(_))));
 tx.rollback().unwrap();
 // A missing FK or an enabled trigger makes the collection replacement fail closed.
 let set_table=rustorm::schema::implicit_join_name("User","Tag");
 let mut tx=db.begin().unwrap();
 tx.execute_statement(&format!("ALTER TABLE \"{}\" DROP CONSTRAINT \"{}_a_fkey\"",set_table,set_table)).unwrap();
 assert!(matches!(tx.nested_set(NestedSet::clear_through(user::tags(),id).unwrap()),Err(Error::InvalidInput(_))));
 tx.rollback().unwrap();
 db.execute_raw(&format!("CREATE FUNCTION skip_set() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER skip_set BEFORE DELETE ON \"{}\" FOR EACH ROW EXECUTE FUNCTION skip_set()",set_table)).unwrap();
 assert!(matches!(db.nested_set(NestedSet::clear_through(user::tags(),id).unwrap()),Err(Error::InvalidInput(_))));
 db.execute_raw(&format!("DROP TRIGGER skip_set ON \"{}\"; DROP FUNCTION skip_set()",set_table)).unwrap();
 // Do not cascade into external tables that reference the implicit link table.
 db.execute_raw(&format!("CREATE TABLE external_link (a text, b integer, FOREIGN KEY (a,b) REFERENCES \"{}\" (a,b) ON DELETE CASCADE)",set_table)).unwrap();
 assert!(matches!(db.nested_set(NestedSet::clear_through(user::tags(),id).unwrap()),Err(Error::InvalidInput(_))));
 db.execute_raw("DROP TABLE external_link").unwrap();
 // Concurrent replacements on one parent serialize to one complete collection.
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let handles=["rust","db"].into_iter().map(|key| {let barrier=barrier.clone();let user=user.clone();let dbname=dbname.clone();
 std::thread::spawn(move || { let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();barrier.wait(); db.nested_set(NestedSet::through(user::tags(),id,vec![key.to_owned()]).unwrap()) }) }).collect::<Vec<_>>();
 assert!(handles.into_iter().all(|h| h.join().unwrap().is_ok()));
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),1);
 db.nested_set(NestedSet::through(user::tags(),id,vec!["rust".to_owned(),"db".to_owned()]).unwrap()).unwrap();
 // If ICU is available, reject distinct input spellings that identify the same database key.
 if db.execute_raw("CREATE COLLATION set_casefold (provider=icu, locale='und-u-ks-level2', deterministic=false)").is_ok() {
  db.execute_raw("CREATE TABLE set_parents (id int PRIMARY KEY); CREATE TABLE set_targets (id text COLLATE set_casefold PRIMARY KEY); CREATE TABLE set_links (p int NOT NULL REFERENCES set_parents(id), c text COLLATE set_casefold NOT NULL REFERENCES set_targets(id), PRIMARY KEY(p,c)); INSERT INTO set_parents VALUES (1); INSERT INTO set_targets VALUES ('Rust'); INSERT INTO set_links VALUES (1,'Rust')").unwrap();
  struct TestParent; struct TestTarget;
  let relation=rustorm::relations::Through::<TestParent,TestTarget,i32,String>::new("set_parents","id","set_targets","id","set_links","p","c",&["id"]);
  let plan=NestedSet::through(relation,1,vec!["Rust".to_owned(),"rust".to_owned()]).unwrap();
  assert!(matches!(db.nested_set(plan),Err(Error::InvalidInput(_))));
  let rows=db.query(&rustorm::query::CompiledQuery {sql:"SELECT 1 FROM set_links".into(),params:vec![]}).unwrap(); assert_eq!(rows.len(),1);
  db.execute_raw("DROP TABLE set_links; DROP TABLE set_targets; DROP TABLE set_parents; DROP COLLATION set_casefold").unwrap();
 }
 // Existing N–N pairs: connect is idempotent, disconnect requires the pair and preserves endpoints.
 db.nested_change(NestedChange::connect_through(user::tags(),id2,"rust").unwrap()).unwrap();
 db.nested_change(NestedChange::connect_through(user::tags(),id2,"rust").unwrap()).unwrap();
 assert!(matches!(db.nested_change(NestedChange::delete_through(user::tags(),id2,"rust",tag::delete_many()).unwrap()),Err(Error::RelationConflict)));
 // The target is shared: update is scoped to an existing pair, and preserves both links.
 let wrong=NestedChange::update_through(user::tags(),first_parent,"rust",tag::update_many().set(tag::NAME,"wrong")).unwrap();
 assert!(matches!(db.nested_change(wrong),Err(Error::RelationNotFound)));
 let mut tx=db.begin().unwrap();
 tx.nested_change(NestedChange::update_through(user::tags(),id2,"rust",tag::update_many().set(tag::NAME,"temporary")).unwrap()).unwrap();
 let q=tag::find_many().where_(tag::ID.equals("rust")).compile().unwrap();
 assert_eq!(tx.fetch::<tag::Record>(&q).unwrap()[0].name,"temporary");
 // A concurrent disconnect cannot remove the verified link while the transaction is open.
 let mut other=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();
 other.execute_raw("SET lock_timeout = '100ms'").unwrap();
 assert!(other.nested_change(NestedChange::disconnect_through(user::tags(),id2,"rust").unwrap()).is_err());
 tx.rollback().unwrap();
 assert_eq!(db.fetch::<tag::Record>(&q).unwrap()[0].name,"");
 db.nested_change(NestedChange::update_through(user::tags(),id2,"rust",tag::update_many().set(tag::NAME,"shared")).unwrap()).unwrap();
 assert_eq!(db.fetch::<tag::Record>(&q).unwrap()[0].name,"shared");
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),2);
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id2)),user::tags()).unwrap()[0].children.len(),1);
 let mut tx=db.begin().unwrap();
 tx.nested_change(NestedChange::disconnect_through(user::tags(),id,"rust").unwrap()).unwrap();
 assert_eq!(tx.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),1);
 tx.rollback().unwrap();
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),2);
 db.nested_change(NestedChange::disconnect_through(user::tags(),id,"rust").unwrap()).unwrap();
 assert!(matches!(db.nested_change(NestedChange::disconnect_through(user::tags(),id,"rust").unwrap()),Err(Error::RelationNotFound)));
 assert_eq!(db.include_through(user::find_many().where_(user::ID.equals(id2)),user::tags()).unwrap()[0].children.len(),1);
 let mut tx=db.begin().unwrap();
 assert!(tx.nested_change(NestedChange::connect_through(user::tags(),id,"missing").unwrap()).is_err());
 tx.nested_change(NestedChange::connect_through(user::tags(),id,"rust").unwrap()).unwrap();
 tx.commit().unwrap();
 // Concurrency on the same pair: idempotent connect, one successful disconnect.
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let handles=(0..2).map(|_| {let barrier=barrier.clone();let user=user.clone();let dbname=dbname.clone();
 std::thread::spawn(move || { let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap(); barrier.wait(); db.nested_change(NestedChange::connect_through(user::tags(),id2,"db").unwrap()) })}).collect::<Vec<_>>();
 assert!(handles.into_iter().all(|h|h.join().unwrap().is_ok()));
 let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
 let handles=(0..2).map(|_| {let barrier=barrier.clone();let user=user.clone();let dbname=dbname.clone();
 std::thread::spawn(move || { let mut db=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap(); barrier.wait(); db.nested_change(NestedChange::disconnect_through(user::tags(),id2,"db").unwrap()) })}).collect::<Vec<_>>();
 let outcomes=handles.into_iter().map(|h|h.join().unwrap()).collect::<Vec<_>>();
 assert_eq!(outcomes.iter().filter(|r|r.is_ok()).count(),1);
 assert_eq!(outcomes.iter().filter(|r|matches!(r,Err(Error::RelationNotFound))).count(),1);
 // An insert-suppressing trigger must not be mistaken for an existing pair.
 let table=rustorm::schema::implicit_join_name("User","Tag");
 db.execute_raw(&format!("CREATE FUNCTION skip_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER skip_link BEFORE INSERT ON \"{}\" FOR EACH ROW EXECUTE FUNCTION skip_link()",table)).unwrap();
 assert!(matches!(db.nested_change(NestedChange::connect_through(user::tags(),id2,"db").unwrap()),Err(Error::RelationNotFound)));
 db.execute_raw(&format!("DROP TRIGGER skip_link ON \"{}\"; DROP FUNCTION skip_link()",table)).unwrap();
 // Create a new N–N target and link it to an existing parent via the inverse descriptor.
 let created=NestedCreate::connect(tag::create().set(tag::ID,"new-tag"),tag::users(),vec![id2]).unwrap();
 assert_eq!(db.nested_create(created).unwrap(),"new-tag");
 assert!(db.include_through(user::find_many().where_(user::ID.equals(id2)),user::tags()).unwrap()[0].children.iter().any(|t|t.id=="new-tag"));
 assert!(matches!(db.nested_change(NestedChange::delete_through(user::tags(),first_parent,"new-tag",tag::delete_many()).unwrap()),Err(Error::RelationNotFound)));
 let mut tx=db.begin().unwrap();
 tx.execute_statement("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").unwrap();
 assert!(matches!(tx.nested_change(NestedChange::delete_through(user::tags(),id2,"new-tag",tag::delete_many()).unwrap()),Err(Error::InvalidInput(_))));
 tx.rollback().unwrap();
 // A role subject to row-level security cannot safely establish exclusivity.
 let superuser = db.query(&rustorm::query::CompiledQuery { sql:"SELECT rolsuper FROM pg_roles WHERE rolname = current_user".into(), params:vec![] }).unwrap()[0].get::<bool>("rolsuper").unwrap();
 if superuser {
  let role=format!("{}_rls",dbname);
  db.execute_raw(&format!("CREATE ROLE \"{}\" NOLOGIN; GRANT USAGE ON SCHEMA public TO \"{}\"; GRANT SELECT, UPDATE ON users, tags TO \"{}\"; GRANT SELECT, INSERT, UPDATE, DELETE ON \"{}\" TO \"{}\"; ALTER TABLE \"{}\" ENABLE ROW LEVEL SECURITY",role,role,role,table,role,table)).unwrap();
  let mut tx=db.begin().unwrap();
  tx.execute_statement(&format!("SET LOCAL ROLE \"{}\"",role)).unwrap();
  assert!(matches!(tx.nested_change(NestedChange::delete_through(user::tags(),id2,"new-tag",tag::delete_many()).unwrap()),Err(Error::InvalidInput(_))));
  assert!(matches!(tx.nested_set(NestedSet::clear_through(user::tags(),id2).unwrap()),Err(Error::InvalidInput(_))));
  tx.rollback().unwrap();
  db.execute_raw(&format!("ALTER TABLE \"{}\" DISABLE ROW LEVEL SECURITY; DROP OWNED BY \"{}\"; DROP ROLE \"{}\"",table,role,role)).unwrap();
 }
 let mut tx=db.begin().unwrap();
 tx.nested_change(NestedChange::delete_through(user::tags(),id2,"new-tag",tag::delete_many()).unwrap()).unwrap();
 let mut other=Connection::connect_local_unencrypted(port,&user,&dbname).unwrap();
 other.execute_raw("SET lock_timeout = '100ms'").unwrap();
 assert!(other.nested_change(NestedChange::connect_through(user::tags(),id,"new-tag").unwrap()).is_err());
 tx.rollback().unwrap();
 let q=tag::find_many().where_(tag::ID.equals("new-tag")).compile().unwrap();
 assert_eq!(db.fetch::<tag::Record>(&q).unwrap().len(),1);
 db.nested_change(NestedChange::delete_through(user::tags(),id2,"new-tag",tag::delete_many()).unwrap()).unwrap();
 assert!(db.fetch::<tag::Record>(&q).unwrap().is_empty());
 let bad=NestedCreate::connect(tag::create().set(tag::ID,"orphan"),tag::users(),vec![i32::MAX]).unwrap();
 assert!(db.nested_create(bad).is_err());
 let q=tag::find_many().where_(tag::ID.equals("orphan")).compile().unwrap();
 assert!(db.fetch::<tag::Record>(&q).unwrap().is_empty());
 let mut tx=db.begin().unwrap();
 tx.nested_create(NestedCreate::connect(tag::create().set(tag::ID,"temporary"),tag::users(),vec![id2]).unwrap()).unwrap();
 tx.rollback().unwrap();
 let q=tag::find_many().where_(tag::ID.equals("temporary")).compile().unwrap();
 assert!(db.fetch::<tag::Record>(&q).unwrap().is_empty());
 assert_eq!(count(&mut db),(baseline.0+1,baseline.1));
 let missing=NestedCreate::connect(user::create(),user::tags(),vec!["rust".to_owned(),"missing".to_owned()]).unwrap();
 assert!(db.nested_create(missing).is_err());
 assert_eq!(count(&mut db),(baseline.0+1,baseline.1));
 let mut tx=db.begin().unwrap();
 let id=tx.nested_create(NestedCreate::connect(user::create(),user::tags(),vec!["db".to_owned()]).unwrap()).unwrap();
 assert_eq!(tx.include_through(user::find_many().where_(user::ID.equals(id)),user::tags()).unwrap()[0].children.len(),1);
 tx.rollback().unwrap();
 assert_eq!(count(&mut db),(baseline.0+1,baseline.1));
 let mut tx=db.begin().unwrap();
 tx.nested_change(NestedChange::delete(user::posts(),first_parent,post::ID.equals(2),post::delete_many()).unwrap()).unwrap();
 tx.rollback().unwrap();
 assert_eq!(count(&mut db),(baseline.0+1,baseline.1));
 db.nested_change(NestedChange::delete(user::posts(),first_parent,post::ID.equals(2),post::delete_many()).unwrap()).unwrap();
 assert_eq!(count(&mut db),(baseline.0+1,baseline.1-1));
 assert_eq!(db.fetch::<user::Record>(&user::find_many().where_(user::ID.equals(first_parent)).compile().unwrap()).unwrap().len(),1);
}
"#).unwrap();
    let out =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&consumer)
            .args(["run", "--offline", "--quiet"])
            .env("RUSTORM_NESTED_DB", &name)
            .env("RUSTORM_TEST_DATABASE_PORT", port.to_string())
            .env("RUSTORM_TEST_DATABASE_USER", &user)
            .env("RUSTORM_TEST_DATABASE_NAME", &admin_db)
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
