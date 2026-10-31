use rustorm::{
    query::{Field, Insert, QueryError},
    relations::{InverseOne, Many, NestedCreate},
};
struct Parent;
struct Child;
const P_ID: Field<i32> = Field::new("parents", "id");
const C_ID: Field<i32> = Field::new("children", "id");
const C_PARENT: Field<i32> = Field::new("children", "parent_id");
const OTHER: Field<i32> = Field::new("other", "id");
fn parent() -> Insert {
    Insert::new("parents", &["id"]).set(P_ID, 1)
}
fn child(n: i32) -> Insert {
    Insert::new("children", &["id", "parent_id"]).set(C_ID, n)
}
fn many() -> Many<Parent, Child, i32> {
    Many::new(
        "parents",
        "id",
        "children",
        "parent_id",
        &["id", "parent_id"],
    )
}
fn one() -> InverseOne<Parent, Child, i32> {
    InverseOne::new(
        "parents",
        "id",
        "children",
        "parent_id",
        &["id", "parent_id"],
    )
}
#[test]
fn validates_nested_parent_and_children_before_execution() {
    assert!(NestedCreate::many(parent(), many(), vec![child(1), child(2)]).is_ok());
    assert!(NestedCreate::one(parent(), one(), child(3)).is_ok());
    assert!(matches!(
        NestedCreate::many(Insert::new("wrong", &[]), many(), vec![child(1)]),
        Err(QueryError::WrongModel)
    ));
    assert!(matches!(
        NestedCreate::many(
            parent(),
            many(),
            vec![Insert::new("other", &[]).set(OTHER, 1)]
        ),
        Err(QueryError::WrongModel)
    ));
    assert!(matches!(
        NestedCreate::many(parent(), many(), vec![child(1).set(C_PARENT, 42)]),
        Err(QueryError::DuplicateField)
    ));
    assert!(matches!(
        NestedCreate::many(
            parent(),
            many(),
            vec![Insert::new("children", &["id", "parent_id"])]
        ),
        Err(QueryError::MissingRequiredField("id"))
    ));
    assert!(NestedCreate::many(parent().returning(&["id"]), many(), vec![child(1)]).is_err());
    assert!(NestedCreate::many(parent(), many(), vec![child(1).returning(&["id"])]).is_err());
    assert!(NestedCreate::many(parent(), many(), vec![]).is_err());
}
#[test]
fn rejects_excessive_child_counts_and_wrong_fields() {
    let children = (0..1001).map(child).collect();
    assert!(matches!(
        NestedCreate::many(parent(), many(), children),
        Err(QueryError::TooManyNestedWrites)
    ));
    assert!(matches!(
        NestedCreate::many(parent(), many(), vec![child(1).set(OTHER, 7)]),
        Err(QueryError::WrongModel)
    ));
    assert!(matches!(
        NestedCreate::one(Insert::new("parents", &["id"]), one(), child(1)),
        Err(QueryError::MissingRequiredField("id"))
    ));
}
#[test]
fn validates_nested_many_to_many_connect_before_execution() {
    use rustorm::relations::Through;
    struct Tag;
    fn tags() -> Through<Parent, Tag, i32, String> {
        Through::new("parents", "id", "tags", "id", "links", "a", "b", &["id"])
    }
    assert!(
        NestedCreate::connect(parent(), tags(), vec!["rust".to_owned(), "db".to_owned()]).is_ok()
    );
    assert!(NestedCreate::connect(parent(), tags(), Vec::<String>::new()).is_err());
    assert!(
        NestedCreate::connect(parent(), tags(), vec!["dup".to_owned(), "dup".to_owned()]).is_err()
    );
    assert!(
        NestedCreate::connect(Insert::new("other", &[]), tags(), vec!["rust".to_owned()]).is_err()
    );
}
#[test]
fn validates_scoped_child_update_and_delete() {
    use rustorm::{
        query::{Delete, Update},
        relations::NestedChange,
    };
    const TITLE: Field<String> = Field::new("children", "title");
    assert!(NestedChange::update(
        many(),
        1,
        C_ID.equals(2),
        Update::new("children").set(TITLE, "new")
    )
    .is_ok());
    assert!(NestedChange::delete(many(), 1, C_ID.equals(2), Delete::new("children")).is_ok());
    assert!(NestedChange::update_one(
        one(),
        1,
        C_ID.equals(2),
        Update::new("children").set(TITLE, "new")
    )
    .is_ok());
    assert!(NestedChange::delete_one(one(), 1, C_ID.equals(2), Delete::new("children")).is_ok());
    assert!(NestedChange::update(
        many(),
        1,
        C_ID.equals(2),
        Update::new("children").set(C_PARENT, 9)
    )
    .is_err());
    assert!(NestedChange::update(
        many(),
        1,
        OTHER.equals(2),
        Update::new("children").set(TITLE, "new")
    )
    .is_err());
    assert!(NestedChange::delete(many(), 1, C_ID.equals(2), Delete::new("other")).is_err());
    assert!(NestedChange::delete(
        many(),
        1,
        C_ID.equals(2),
        Delete::new("children").returning(&["id"])
    )
    .is_err());
}
#[test]
fn disconnect_validates_nullable_fk_and_model_before_any_io() {
    use rustorm::{
        query::QueryError,
        relations::{NestedChange, One},
    };
    let optional_many = || many().with_nullable_fk().with_unique_child_fields(&["id"]);
    let optional_one = || one().with_nullable_fk().with_unique_child_fields(&["id"]);
    let optional_owner = || {
        One::<Child, Parent, i32>::new("children", "parent_id", "parents", "id", true, &["id"])
            .with_unique_child_fields(&["id"])
    };
    let required_owner =
        || One::<Child, Parent, i32>::new("children", "parent_id", "parents", "id", false, &["id"]);
    assert!(NestedChange::disconnect(optional_many(), 1, C_ID.equals(3)).is_ok());
    assert!(matches!(
        NestedChange::disconnect(optional_many(), 1, C_ID.gte(1)),
        Err(QueryError::NonUniqueNestedSelector)
    ));
    assert!(matches!(
        NestedChange::disconnect(optional_many(), 1, C_PARENT.equals(1)),
        Err(QueryError::NonUniqueNestedSelector)
    ));
    assert!(matches!(
        NestedChange::disconnect(
            optional_many(),
            1,
            rustorm::query::Filter::or(vec![C_ID.equals(1), C_ID.equals(2)])
        ),
        Err(QueryError::NonUniqueNestedSelector)
    ));
    assert!(NestedChange::disconnect(
        optional_many(),
        1,
        rustorm::query::Filter::and(vec![C_ID.equals(1), C_PARENT.equals(1)])
    )
    .is_ok());
    assert!(NestedChange::disconnect_one(optional_one(), 1, C_ID.equals(3)).is_ok());
    assert!(NestedChange::disconnect_owner(optional_owner(), 1, C_ID.equals(3)).is_ok());
    assert!(matches!(
        NestedChange::disconnect(many(), 1, C_ID.equals(3)),
        Err(QueryError::RequiredRelationViolation)
    ));
    assert!(matches!(
        NestedChange::disconnect_one(one(), 1, C_ID.equals(3)),
        Err(QueryError::RequiredRelationViolation)
    ));
    assert!(matches!(
        NestedChange::disconnect_owner(required_owner(), 1, C_ID.equals(3)),
        Err(QueryError::RequiredRelationViolation)
    ));
    assert!(matches!(
        NestedChange::disconnect(optional_many(), 1, OTHER.equals(3)),
        Err(QueryError::WrongModel)
    ));
}

#[test]
fn disconnect_requires_every_component_of_a_composite_key() {
    use rustorm::{query::Filter, relations::NestedChange};
    const C_SECOND: Field<i32> = Field::new("children", "second");
    let rel = || {
        many()
            .with_nullable_fk()
            .with_composite_child_key(&["id", "second"])
    };
    assert!(NestedChange::disconnect(
        rel(),
        1,
        Filter::and(vec![C_ID.equals(4), C_SECOND.equals(2)])
    )
    .is_ok());
    assert!(matches!(
        NestedChange::disconnect(rel(), 1, C_ID.equals(4)),
        Err(QueryError::NonUniqueNestedSelector)
    ));
    assert!(matches!(
        NestedChange::disconnect(
            rel(),
            1,
            Filter::and(vec![
                C_ID.equals(4),
                Filter::or(vec![C_SECOND.equals(2), C_SECOND.equals(3)])
            ])
        ),
        Err(QueryError::NonUniqueNestedSelector)
    ));
}

#[test]
fn nested_many_to_many_links_validate_null_keys() {
    use rustorm::{
        query::Value,
        relations::{NestedChange, Through},
    };
    struct Tag;
    fn tags() -> Through<Parent, Tag, i32, String> {
        Through::new("parents", "id", "tags", "id", "links", "a", "b", &["id"])
    }
    struct NullKey;
    impl rustorm::query::IntoFieldValue<i32> for NullKey {
        fn into_value(self) -> Value {
            Value::Null
        }
    }
    assert!(NestedChange::connect_through(tags(), 1, "rust").is_ok());
    assert!(NestedChange::disconnect_through(tags(), 1, "rust").is_ok());
    assert!(matches!(
        NestedChange::connect_through(tags(), NullKey, "rust"),
        Err(QueryError::NullInValueList)
    ));
    assert!(matches!(
        NestedChange::disconnect_through(tags(), NullKey, "rust"),
        Err(QueryError::NullInValueList)
    ));
}

#[test]
fn nested_many_to_many_update_validates_target_and_prevents_key_mutation() {
    use rustorm::{
        query::{Update, Value},
        relations::{NestedChange, Through},
    };
    struct Tag;
    fn tags() -> Through<Parent, Tag, i32, String> {
        Through::new(
            "parents",
            "id",
            "tags",
            "id",
            "links",
            "a",
            "b",
            &["id", "name"],
        )
    }
    const TAG_ID: Field<String> = Field::new("tags", "id");
    const TAG_NAME: Field<String> = Field::new("tags", "name");
    struct NullKey;
    impl rustorm::query::IntoFieldValue<i32> for NullKey {
        fn into_value(self) -> Value {
            Value::Null
        }
    }
    assert!(NestedChange::update_through(
        tags(),
        1,
        "rust",
        Update::new("tags").set(TAG_NAME, "Rust")
    )
    .is_ok());
    assert!(matches!(
        NestedChange::update_through(tags(), 1, "rust", Update::new("tags").set(TAG_ID, "other")),
        Err(QueryError::NestedRelationKeyMutation)
    ));
    assert!(matches!(
        NestedChange::update_through(tags(), 1, "rust", Update::new("other").set(TAG_NAME, "x")),
        Err(QueryError::WrongModel)
    ));
    assert!(matches!(
        NestedChange::update_through(
            tags(),
            NullKey,
            "rust",
            Update::new("tags").set(TAG_NAME, "x")
        ),
        Err(QueryError::NullInValueList)
    ));
}

#[test]
fn nested_many_to_many_delete_checks_model_and_null_keys() {
    use rustorm::{
        query::{Delete, Value},
        relations::{NestedChange, Through},
    };
    struct Tag;
    fn tags() -> Through<Parent, Tag, i32, String> {
        Through::new("parents", "id", "tags", "id", "links", "a", "b", &["id"])
    }
    struct NullKey;
    impl rustorm::query::IntoFieldValue<i32> for NullKey {
        fn into_value(self) -> Value {
            Value::Null
        }
    }
    assert!(NestedChange::delete_through(tags(), 1, "rust", Delete::new("tags")).is_ok());
    assert!(matches!(
        NestedChange::delete_through(tags(), 1, "rust", Delete::new("other")),
        Err(QueryError::WrongModel)
    ));
    assert!(matches!(
        NestedChange::delete_through(tags(), NullKey, "rust", Delete::new("tags")),
        Err(QueryError::NullInValueList)
    ));
    assert!(matches!(
        NestedChange::delete_through(tags(), 1, "rust", Delete::new("tags").returning(&["id"])),
        Err(QueryError::NestedReturningUnsupported)
    ));
}

#[test]
fn nested_set_requires_explicit_clear_and_unique_non_null_keys() {
    use rustorm::{
        query::Value,
        relations::{NestedSet, Through},
    };
    struct Tag;
    fn tags() -> Through<Parent, Tag, i32, String> {
        Through::new("parents", "id", "tags", "id", "links", "a", "b", &["id"])
    }
    struct NullKey;
    impl rustorm::query::IntoFieldValue<i32> for NullKey {
        fn into_value(self) -> Value {
            Value::Null
        }
    }
    assert!(NestedSet::through(tags(), 1, vec!["one".to_owned(), "two".to_owned()]).is_ok());
    assert!(matches!(
        NestedSet::through(tags(), 1, Vec::<String>::new()),
        Err(QueryError::EmptyNestedWrite)
    ));
    assert!(matches!(
        NestedSet::through(tags(), 1, vec!["a".to_owned(), "a".to_owned()]),
        Err(QueryError::DuplicateNestedTarget)
    ));
    assert!(matches!(
        NestedSet::through(tags(), 1, vec!["x".to_owned(); 1001]),
        Err(QueryError::TooManyNestedWrites)
    ));
    assert!(matches!(
        NestedSet::clear_through(tags(), NullKey),
        Err(QueryError::NullInValueList)
    ));
    assert!(NestedSet::clear_through(tags(), 1).is_ok());
}

#[test]
fn nested_set_many_checks_fk_and_unique_target_before_io() {
    use rustorm::relations::NestedSet;
    const NOT_UNIQUE: Field<i32> = Field::new("children", "parent_id");
    let optional = || many().with_nullable_fk().with_unique_child_fields(&["id"]);
    assert!(NestedSet::many(optional(), 1, C_ID, vec![3, 4]).is_ok());
    assert!(NestedSet::clear_many(optional(), 1, C_ID).is_ok());
    assert!(matches!(
        NestedSet::many(many(), 1, C_ID, vec![3]),
        Err(QueryError::RequiredRelationViolation)
    ));
    assert!(matches!(
        NestedSet::many(optional(), 1, NOT_UNIQUE, vec![3]),
        Err(QueryError::NonUniqueNestedSelector)
    ));
    assert!(matches!(
        NestedSet::many(optional(), 1, OTHER, vec![3]),
        Err(QueryError::WrongModel)
    ));
    assert!(matches!(
        NestedSet::many(optional(), 1, C_ID, vec![]),
        Err(QueryError::EmptyNestedWrite)
    ));
    assert!(matches!(
        NestedSet::many(optional(), 1, C_ID, vec![3, 3]),
        Err(QueryError::DuplicateNestedTarget)
    ));
}
