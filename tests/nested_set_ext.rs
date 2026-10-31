use rustorm::{
    query::{Field, QueryError},
    relations::{Many, One, InverseOne},
    nested_writes::{ExplicitThrough, NestedSetExt},
};
struct Parent;
struct Child;
const P_ID: Field<i32> = Field::new("parents", "id");
const C_ID: Field<i32> = Field::new("children", "id");
const C_FK: Field<i32> = Field::new("children", "parent_id");
const OTHER: Field<i32> = Field::new("other", "id");

fn many_nullable() -> Many<Parent, Child, i32> {
    Many::new("parents", "id", "children", "parent_id", &["id", "parent_id"])
        .with_nullable_fk()
        .with_unique_child_fields(&["id"])
}
fn many_required() -> Many<Parent, Child, i32> {
    Many::new("parents", "id", "children", "parent_id", &["id", "parent_id"])
        .with_unique_child_fields(&["id"])
}
fn one_owned() -> One<Child, Parent, i32> {
    One::new("children", "parent_id", "parents", "id", true, &["id"])
}
fn one_required() -> One<Child, Parent, i32> {
    One::new("children", "parent_id", "parents", "id", false, &["id"])
}
fn inverse_one() -> InverseOne<Parent, Child, i32> {
    InverseOne::new("parents", "id", "children", "parent_id", &["id", "parent_id"])
        .with_nullable_fk()
        .with_unique_child_fields(&["id"])
}

#[test]
fn one_owned_set_validates_unique_and_null() {
    assert!(NestedSetExt::one_owned(one_owned(), 1, C_ID, vec![10]).is_ok());
    assert!(NestedSetExt::<_,_,i32,i32>::clear_one_owned(one_owned(), 1).is_ok());
    // plus d'une cible pour 1-1 est refusé
    assert!(matches!(NestedSetExt::one_owned(one_owned(), 1, C_ID, vec![1,2]), Err(QueryError::TooManyNestedWrites)));
    // dup
    assert!(matches!(NestedSetExt::one_owned(one_owned(), 1, C_ID, vec![5,5]), Err(QueryError::DuplicateNestedTarget)));
    // clear avec parent null refusé (on teste via valeur null custom)
    struct NullKey; impl rustorm::query::IntoFieldValue<i32> for NullKey { fn into_value(self) -> rustorm::query::Value { rustorm::query::Value::Null } }
    assert!(matches!(NestedSetExt::<_,_,i32,i32>::clear_one_owned(one_owned(), NullKey), Err(QueryError::NullInValueList)));
}

#[test]
fn one_inverse_set() {
    assert!(NestedSetExt::one_inverse(inverse_one(), 1, C_ID, vec![10]).is_ok());
    assert!(NestedSetExt::<_,_,i32,i32>::clear_one_inverse(inverse_one(), 1).is_ok());
}

#[test]
fn mandatory_fk_set_requires_no_detachment_logic() {
    // many_required est autorisé à la construction, l'erreur de détachement sera levée en DB
    assert!(NestedSetExt::many_required(many_required(), 1, C_ID, vec![10, 11]).is_ok());
    assert!(NestedSetExt::clear_many_required(many_required(), 1, C_ID).is_ok());
    // vérifie validation unique
    assert!(matches!(NestedSetExt::many_required(many_required(), 1, OTHER, vec![10]), Err(QueryError::WrongModel)));
    assert!(matches!(NestedSetExt::many_required(many_required(), 1, C_FK, vec![10]), Err(QueryError::NonUniqueNestedSelector)));
}

#[test]
fn composite_key_set_validates_filters() {
    let rel = || Many::<Parent, Child, i32>::new("parents", "id", "children", "parent_id", &["id", "second", "parent_id"])
        .with_nullable_fk()
        .with_unique_child_fields(&["id"])
        .with_composite_child_key(&["id", "second"]);
    use rustorm::query::Filter;
    const C_SECOND: Field<i32> = Field::new("children", "second");
    let ok_filter = Filter::and(vec![C_ID.equals(5), C_SECOND.equals(2)]);
    assert!(NestedSetExt::<_,_,i32, i32>::many_composite(rel(), 1, vec![ok_filter]).is_ok());
    let bad_single = C_ID.equals(5);
    assert!(matches!(NestedSetExt::<_,_,i32,i32>::many_composite(rel(), 1, vec![bad_single]), Err(QueryError::NonUniqueNestedSelector)));
    let or_filter = Filter::or(vec![C_ID.equals(5), C_SECOND.equals(2)]);
    assert!(matches!(NestedSetExt::<_,_,i32,i32>::many_composite(rel(), 1, vec![or_filter]), Err(QueryError::NonUniqueNestedSelector)));
    assert!(NestedSetExt::<_,_,i32,i32>::clear_many_composite(rel(), 1).is_ok());
}

#[test]
fn explicit_nn_set() {
    let rel = ExplicitThrough::<Parent, Child, i32, i32>::new("parents", "id", "children", "id", "post_tags", "parent_id", "child_id", &["parent_id", "child_id"]);
    assert!(NestedSetExt::explicit_through(rel, 1, vec![10, 11]).is_ok());
    // duplicate
    let rel2 = ExplicitThrough::<Parent, Child, i32, i32>::new("parents", "id", "children", "id", "post_tags", "parent_id", "child_id", &["parent_id", "child_id"]);
    assert!(matches!(NestedSetExt::explicit_through(rel2, 1, vec![1,1]), Err(QueryError::DuplicateNestedTarget)));
    let rel3 = ExplicitThrough::<Parent, Child, i32, i32>::new("parents", "id", "children", "id", "post_tags", "parent_id", "child_id", &["parent_id", "child_id"]);
    assert!(NestedSetExt::clear_explicit_through(rel3, 1).is_ok());
}
