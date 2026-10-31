use rustorm::{
    query::{Field, Insert, Update, QueryError},
    relations::{Many, One, InverseOne, Through},
    nested_writes::NestedUpsert,
};
struct Parent;
struct Child;
struct Tag;
const C_ID: Field<i32> = Field::new("children", "id");
const C_TITLE: Field<String> = Field::new("children", "title");
const P_ID: Field<i32> = Field::new("parents", "id");

fn many() -> Many<Parent, Child, i32> {
    Many::new("parents", "id", "children", "parent_id", &["id", "parent_id", "title"])
        .with_nullable_fk()
        .with_unique_child_fields(&["id"])
}

fn through() -> Through<Parent, Tag, i32, String> {
    Through::new("parents", "id", "tags", "id", "links", "a", "b", &["id"])
}

#[test]
fn upsert_many_validates_unique_selector() {
    let sel_ok = C_ID.equals(5);
    let create = Insert::new("children", &["id"]).set(C_ID, 5).set(C_TITLE, "hi".to_owned());
    let update = Update::new("children").set(C_TITLE, "bye".to_owned());
    assert!(NestedUpsert::many(many(), 1, sel_ok, create, update).is_ok());

    let sel_bad = C_TITLE.equals("hi".to_owned()); // not unique
    let create2 = Insert::new("children", &["id"]).set(C_ID, 5);
    let update2 = Update::new("children").set(C_TITLE, "x".to_owned());
    assert!(matches!(NestedUpsert::many(many(), 1, sel_bad, create2, update2), Err(QueryError::NonUniqueNestedSelector)));

    // wrong model
    let sel_ok2 = C_ID.equals(10);
    let bad_create = Insert::new("other", &[]).set(Field::<i32>::new("other","id"), 1);
    let upd = Update::new("children").set(C_TITLE, "a".to_owned());
    assert!(matches!(NestedUpsert::many(many(), 1, sel_ok2, bad_create, upd), Err(QueryError::WrongModel)));
}

#[test]
fn upsert_one_and_inverse() {
    let one = One::<Child, Parent, i32>::new("children", "parent_id", "parents", "id", true, &["id"]);
    let sel = C_ID.equals(3);
    let create = Insert::new("children", &["id"]).set(C_ID, 3);
    let update = Update::new("children").set(C_TITLE, "u".to_owned());
    assert!(NestedUpsert::one(one, 1, sel, create, update).is_ok());

    let inv = InverseOne::<Parent, Child, i32>::new("parents", "id", "children", "parent_id", &["id", "parent_id"]);
    let sel2 = C_ID.equals(4);
    let create2 = Insert::new("children", &["id"]).set(C_ID, 4);
    let update2 = Update::new("children").set(C_TITLE, "v".to_owned());
    assert!(NestedUpsert::inverse_one(inv, 1, sel2, create2, update2).is_ok());
}

#[test]
fn upsert_through() {
    let sel = Field::<String>::new("tags", "id").equals("rust".to_owned());
    let create = Insert::new("tags", &["id"]).set(Field::<String>::new("tags","id"), "rust".to_owned());
    let update = Update::new("tags").set(Field::<String>::new("tags","name"), "Rust".to_owned());
    assert!(NestedUpsert::through(through(), 1, sel, create, update).is_ok());
}
