use rustorm::{
    query::{Field, Insert, QueryError, Filter},
    relations::{Many, Through},
    nested_writes::{NestedConnectOrCreate, ExplicitThrough},
};
struct Parent;
struct Child;
struct Tag;
const C_ID: Field<i32> = Field::new("children", "id");
const C_EMAIL: Field<String> = Field::new("children", "email");

fn many() -> Many<Parent, Child, i32> {
    Many::new("parents", "id", "children", "parent_id", &["id", "parent_id", "email"])
        .with_nullable_fk()
        .with_unique_child_fields(&["email"])
}

fn through() -> Through<Parent, Tag, i32, String> {
    Through::new("parents", "id", "tags", "id", "links", "a", "b", &["id"])
}

#[test]
fn connect_or_create_many_validates_unique() {
    let sel = C_EMAIL.equals("a@b".to_owned());
    let create = Insert::new("children", &["email"]).set(C_EMAIL, "a@b".to_owned());
    assert!(NestedConnectOrCreate::many(many(), 1, sel, create).is_ok());

    let bad_sel = Field::<String>::new("children", "title").equals("nope".to_owned());
    let create2 = Insert::new("children", &["email"]).set(C_EMAIL, "x".to_owned());
    assert!(matches!(NestedConnectOrCreate::many(many(), 1, bad_sel, create2), Err(QueryError::NonUniqueNestedSelector)));
}

#[test]
fn connect_or_create_through() {
    let sel = Field::<String>::new("tags", "id").equals("rust".to_owned());
    let create = Insert::new("tags", &["id"]).set(Field::<String>::new("tags","id"), "rust".to_owned());
    assert!(NestedConnectOrCreate::through_implicit(through(), 1, sel, create).is_ok());
}

#[test]
fn connect_or_create_explicit_nn() {
    let rel = ExplicitThrough::<Parent, Tag, i32, String>::new("parents","id","tags","id","post_tags","parent_id","child_id",&["parent_id","child_id"]);
    let sel = Field::<String>::new("tags","id").equals("a".to_owned());
    let create = Insert::new("tags",&["id"]).set(Field::<String>::new("tags","id"), "a".to_owned());
    assert!(NestedConnectOrCreate::through_explicit(rel, 1, sel, create).is_ok());
}
