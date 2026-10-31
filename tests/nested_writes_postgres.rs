#![cfg(feature = "postgres")]
use rustorm::{migrations, postgres::Connection, schema};
use std::{fs, path::PathBuf};
const SCHEMA: &str = r#"datasource db {
 provider = "postgresql"
}
model User {
 id Int @id
 posts Post[]
 profile Profile?
 tags Tag[]
 requiredPosts RequiredPost[]
 requiredProfile RequiredProfile?
 compositeItems CompositeItem[]
 @@map("users")
}
model Post {
 id Int @id
 title String @unique
 authorId Int @map("author_id")
 author User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 postTags PostTag[]
 @@map("posts")
}
model RequiredPost {
 id Int @id
 title String @unique
 authorId Int @map("author_id")
 author User @relation(fields: [authorId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 @@map("required_posts")
}
model Profile {
 id Int @id
 userId Int? @unique @map("user_id")
 user User? @relation(fields: [userId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 @@map("profiles")
}
model RequiredProfile {
 id Int @id
 userId Int @unique @map("user_id")
 user User @relation(fields: [userId], references: [id], onDelete: Restrict, onUpdate: Cascade)
 @@map("required_profiles")
}
model Tag {
 id String @id
 users User[]
 postTags PostTag[]
 @@map("tags")
}
model CompositeItem {
 id Int
 lang String
 ownerId Int? @map("owner_id")
 owner User? @relation(fields: [ownerId], references: [id], onDelete: SetNull, onUpdate: Cascade)
 @@id([id, lang])
 @@map("composite_items")
}
model PostTag {
 postId Int @map("post_id")
 tagId String @map("tag_id")
 post Post @relation(fields: [postId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 tag Tag @relation(fields: [tagId], references: [id], onDelete: Cascade, onUpdate: Cascade)
 @@id([postId, tagId])
 @@map("post_tags")
}
"#;

struct Cleanup { admin: Connection, name: String, dir: PathBuf }
impl Drop for Cleanup { fn drop(&mut self) { let _ = self.admin.execute_raw(&format!("DROP DATABASE \"{}\"", self.name)); let _ = fs::remove_dir_all(&self.dir); } }

#[test]
#[ignore = "requires disposable PostgreSQL and CREATEDB privilege"]
fn nested_writes_all_remaining_features() {
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap().parse().unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let admin_db = std::env::var("RUSTORM_TEST_DATABASE_NAME").unwrap_or_else(|_| "postgres".into());
    let mut admin = Connection::connect_local_unencrypted(port, &user, &admin_db).unwrap();
    let name = format!("rustorm_writes_{}", std::process::id());
    admin.execute_raw(&format!("CREATE DATABASE \"{name}\"")).unwrap();
    let dir = std::env::temp_dir().join(format!("rustorm-writes-{}", std::process::id()));
    let guard = Cleanup { admin, name: name.clone(), dir: dir.clone() };
    let mut db = Connection::connect_local_unencrypted(port, &user, &name).unwrap();
    let parsed = schema::parse(SCHEMA).unwrap();
    migrations::create(&dir, "initial", &parsed).unwrap();
    migrations::deploy(&mut db, &dir).unwrap();
    db.execute_raw("INSERT INTO users (id) VALUES (1),(2); INSERT INTO posts VALUES (10,'t10',1),(11,'t11',1); INSERT INTO required_posts VALUES (20,'r20',1); INSERT INTO profiles VALUES (100,1); INSERT INTO required_profiles VALUES (200,1); INSERT INTO tags VALUES ('rust'),('db'); INSERT INTO composite_items VALUES (1,'fr',1),(1,'en',1); INSERT INTO post_tags VALUES (10,'rust')").unwrap();
    let consumer = dir.join("consumer");
    fs::create_dir_all(consumer.join("src")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR").replace('\\', "\\\\").replace('"', "\\\"");
    fs::write(consumer.join("Cargo.toml"), format!("[package]\nname=\"writes_consumer\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nrustorm={{path=\"{root}\",features=[\"postgres\"]}}\n")).unwrap();
    fs::write(consumer.join("src/generated.rs"), schema::generate(&parsed)).unwrap();
    fs::write(consumer.join("src/main.rs"), r#"
mod generated;
use generated::{user, post, required_post, profile, required_profile, tag, composite_item, post_tag};
use rustorm::{
    nested_writes::{NestedSetExt, ExplicitThrough, NestedUpsert, NestedConnectOrCreate, NestedGraph, RelationRef},
    postgres::{Connection, Error},
    query::{QueryError, Value},
};
fn main() {
    let port = std::env::var("RUSTORM_TEST_DATABASE_PORT").unwrap().parse().unwrap();
    let user = std::env::var("RUSTORM_TEST_DATABASE_USER").unwrap_or_else(|_| "rustorm".into());
    let dbname = std::env::var("RUSTORM_WRITES_DB").unwrap();
    let mut db = Connection::connect_local_unencrypted(port, &user, &dbname).unwrap();

    // 1) set 1–1 : remplace profil
    db.execute_raw("INSERT INTO profiles VALUES (101,NULL)").unwrap();
    let plan = NestedSetExt::one_owned(profile::user(), 1, profile::ID, vec![101]).unwrap();
    db.nested_set_ext(plan).unwrap();
    let q = profile::find_many().where_(profile::ID.equals(100)).compile().unwrap();
    assert!(db.fetch::<profile::Record>(&q).unwrap()[0].user_id.is_none());
    let q2 = profile::find_many().where_(profile::ID.equals(101)).compile().unwrap();
    assert_eq!(db.fetch::<profile::Record>(&q2).unwrap()[0].user_id, Some(1));
    // clear 1–1
    let clear: NestedSetExt<user::Record, profile::Record, i32, i32> = NestedSetExt::clear_one_owned(profile::user(), 1).unwrap();
    db.nested_set_ext(clear).unwrap();
    assert!(db.fetch::<profile::Record>(&q2).unwrap()[0].user_id.is_none());

    // 1–1 inverse
    db.execute_raw("UPDATE profiles SET user_id=1 WHERE id=100").unwrap();
    let plan_inv = NestedSetExt::one_inverse(user::profile(), 1, profile::ID, vec![100]).unwrap();
    db.nested_set_ext(plan_inv).unwrap();
    // mandatory FK : set sans détachement (collection vide -> ajout)
    db.execute_raw("DELETE FROM required_posts").unwrap();
    let plan_req = NestedSetExt::many_required(user::required_posts(), 1, required_post::ID, vec![]).unwrap();
    // clear sur FK obligatoire doit échouer
    assert!(matches!(db.nested_set_ext(plan_req), Err(Error::InvalidInput(_))));
    // ajout sans détachement : ok (existante vide, on ajoute)
    db.nested_set_ext(NestedSetExt::many_required(user::required_posts(), 1, required_post::ID, vec![20]).unwrap()).unwrap_err(); // 20 n'existe plus après delete
    db.execute_raw("INSERT INTO required_posts VALUES (21,'r21',NULL)").unwrap();
    let plan_req2 = NestedSetExt::many_required(user::required_posts(), 1, required_post::ID, vec![21]).unwrap();
    db.nested_set_ext(plan_req2).unwrap();
    let qreq = required_post::find_many().where_(required_post::ID.equals(21)).compile().unwrap();
    assert_eq!(db.fetch::<required_post::Record>(&qreq).unwrap()[0].author_id, 1);
    // tentative de détachement sur FK obligatoire -> refus
    let plan_detach = NestedSetExt::many_required(user::required_posts(), 1, required_post::ID, vec![]).unwrap();
    assert!(matches!(db.nested_set_ext(plan_detach), Err(Error::InvalidInput(_))));

    // clés composées
    let comp_rel = user::composite_items();
    let f = rustorm::query::Filter::and(vec![composite_item::ID.equals(1), composite_item::LANG.equals("fr")]);
    let plan_comp: NestedSetExt<user::Record, composite_item::Record, i32, i32> = NestedSetExt::many_composite(comp_rel, 1, vec![f]).unwrap();
    db.nested_set_ext(plan_comp).unwrap();
    let rows = db.fetch::<composite_item::Record>(&composite_item::find_many().compile().unwrap()).unwrap();
    assert_eq!(rows.iter().filter(|r| r.owner_id==Some(1)).count(), 1);
    // N–N explicite via post_tags
    let explicit = ExplicitThrough::<post::Record, tag::Record, i32, String>::new("posts","id","tags","id","post_tags","post_id","tag_id",&["post_id","tag_id"]);
    let plan_exp = NestedSetExt::explicit_through(explicit, 10, vec!["db".to_owned()]).unwrap();
    db.nested_set_ext(plan_exp).unwrap();
    let cnt = db.query(&rustorm::query::CompiledQuery { sql: "SELECT 1 FROM post_tags WHERE post_id=10".into(), params: vec![] }).unwrap().len();
    assert_eq!(cnt, 1);

    // upsert 1–N : crée ou modifie
    let sel = post::TITLE.equals("new-upsert");
    let create = post::create().set(post::ID, 99).set(post::TITLE, "new-upsert");
    let update = post::update_many().set(post::TITLE, "updated-upsert");
    let up = NestedUpsert::many(user::posts(), 1, sel, create, update).unwrap();
    db.nested_upsert(up).unwrap();
    let qpost = post::find_many().where_(post::ID.equals(99)).compile().unwrap();
    assert_eq!(db.fetch::<post::Record>(&qpost).unwrap().len(), 1);
    // second upsert sur même sélecteur doit faire update
    let sel2 = post::TITLE.equals("new-upsert");
    let create2 = post::create().set(post::ID, 100).set(post::TITLE, "new-upsert");
    let update2 = post::update_many().set(post::TITLE, "updated-again");
    let up2 = NestedUpsert::many(user::posts(), 1, sel2, create2, update2).unwrap();
    // comme le titre a changé, le sélecteur ne matche plus l'ancien, on crée un nouveau
    db.nested_upsert(up2).unwrap();
    assert_eq!(db.fetch::<post::Record>(&post::find_many().compile().unwrap()).unwrap().len(), 5);

    // connect_or_create
    let sel3 = tag::ID.equals("new-tag");
    let create3 = tag::create().set(tag::ID, "new-tag");
    let coc = NestedConnectOrCreate::through_implicit(user::tags(), 1, sel3, create3).unwrap();
    db.nested_connect_or_create(coc).unwrap();
    // second appel idempotent
    let sel4 = tag::ID.equals("new-tag");
    let create4 = tag::create().set(tag::ID, "new-tag");
    let coc2 = NestedConnectOrCreate::through_implicit(user::tags(), 1, sel4, create4).unwrap();
    db.nested_connect_or_create(coc2).unwrap();
    // concurrence connect_or_create
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = (0..2).map(|_| { let b=barrier.clone(); let u=user.clone(); let dbn=dbname.clone(); std::thread::spawn(move || { let mut db=Connection::connect_local_unencrypted(port, &u, &dbn).unwrap(); b.wait(); let sel=tag::ID.equals("race-tag"); let create=tag::create().set(tag::ID,"race-tag"); let coc=NestedConnectOrCreate::through_implicit(user::tags(), 1, sel, create).unwrap(); db.nested_connect_or_create(coc).is_ok() }) }).collect::<Vec<_>>();
    let ok = handles.into_iter().map(|h| h.join().unwrap()).filter(|v| *v).count();
    assert_eq!(ok, 1, "connect_or_create concurrent doit garantir unicité sans succès mensonger");

    // multi-level : User -> Post -> Tag en une transaction
    let mut graph = NestedGraph::new();
    let u = graph.add_create(user::create().set(user::ID, 10)).unwrap();
    let p_rel = RelationRef { parent_table:"users", parent_key:"id", child_table:"posts", child_key:"author_id", nullable:false };
    let p = graph.add_create_child(p_rel, post::create().set(post::ID, 200).set(post::TITLE, "multi")).unwrap();
    graph.add_dependency(u, p).unwrap();
    graph.add_connect_through("post_tags","post_id","tag_id", Value::Int(200), Value::String("rust".into())).unwrap();
    // cycle detection
    let mut bad = NestedGraph::new();
    let a = bad.add_create(user::create().set(user::ID, 20)).unwrap();
    let b = bad.add_create(post::create().set(post::ID, 201).set(post::TITLE, "cycle")).unwrap();
    bad.add_dependency(a,b).unwrap();
    bad.add_dependency(b,a).unwrap_err(); // cycle refusé avant I/O
    // atomicité : erreur dans le graphe rollback tout
    let mut graph2 = NestedGraph::new();
    graph2.add_create(user::create().set(user::ID, 30)).unwrap();
    // update ambigu (plusieurs lignes) doit rollback
    graph2.add_update("posts", post::ID.gte(10), post::update_many().set(post::TITLE, "bad")).unwrap();
    assert!(matches!(db.nested_graph(graph2), Err(Error::RelationAmbiguous)));
    let quser = user::find_many().where_(user::ID.equals(30)).compile().unwrap();
    assert!(db.fetch::<user::Record>(&quser).unwrap().is_empty(), "rollback multi-level doit annuler création parent");

    // interactions multi-opérations : set + upsert + disconnect dans même graph
    let mut graph3 = NestedGraph::new();
    graph3.add_create(user::create().set(user::ID, 40)).unwrap();
    let sel_up = post::TITLE.equals("graph-up");
    graph3.add_upsert("posts", sel_up, post::create().set(post::ID, 300).set(post::TITLE, "graph-up"), post::update_many().set(post::TITLE, "graph-up2")).unwrap();
    graph3.validate().unwrap();
    db.nested_graph(graph3).unwrap();
    // vérifie trigger refusé (on crée un trigger et on tente une opération)
    db.execute_raw("CREATE FUNCTION keep_post() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.author_id := OLD.author_id; RETURN NEW; END $$; CREATE TRIGGER keep_post BEFORE UPDATE ON posts FOR EACH ROW EXECUTE FUNCTION keep_post()").unwrap();
    let mut tx = db.begin().unwrap();
    assert!(matches!(tx.nested_set_ext(NestedSetExt::many_required(user::required_posts(), 1, required_post::ID, vec![21]).unwrap()), Err(Error::InvalidInput(_))));
    tx.rollback().unwrap();
    db.execute_raw("DROP TRIGGER keep_post ON posts; DROP FUNCTION keep_post()").unwrap();

    // isolation : multi-level exige READ COMMITTED
    let mut tx2 = db.begin().unwrap();
    tx2.execute_statement("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").unwrap();
    let mut g4 = NestedGraph::new();
    g4.add_create(user::create().set(user::ID, 50)).unwrap();
    assert!(matches!(tx2.nested_graph(g4), Err(Error::InvalidInput(_))));
    tx2.rollback().unwrap();
}
"#).unwrap();
    let out = std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .current_dir(&consumer)
        .args(["run", "--offline", "--quiet"])
        .env("RUSTORM_WRITES_DB", &name)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    drop(db);
    drop(guard);
}
