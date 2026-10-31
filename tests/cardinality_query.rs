use rustorm::{
    query::{Field, Select, Value},
    relations::{InverseOne, One, Through},
};
struct User;
struct Profile;
struct Post;
struct Category;
fn through() -> Through<Post, Category, i32, String> {
    Through::new(
        "posts",
        "id",
        "categories",
        "id",
        "post_category",
        "b",
        "a",
        &["id", "name"],
    )
}
#[test]
fn singular_filters_are_correlated_and_parameterized_in_both_directions() {
    let inverse = InverseOne::<User, Profile, i32>::new(
        "users",
        "id",
        "profiles",
        "user_id",
        &["id", "user_id"],
    );
    let sql = Select::new("users", &["id"])
        .where_(inverse.is(Field::<i32>::new("profiles", "id").equals(42)))
        .compile()
        .unwrap();
    assert!(
        sql.sql.contains("EXISTS (SELECT 1 FROM \"profiles\""),
        "{}",
        sql.sql
    );
    assert_eq!(sql.params, vec![Value::Int(42)]);
    let owner =
        One::<Profile, User, i32>::new("profiles", "user_id", "users", "id", false, &["id"]);
    let sql = Select::new("profiles", &["id"])
        .where_(owner.is_not(Field::<i32>::new("users", "id").equals(5)))
        .compile()
        .unwrap();
    assert!(sql.sql.contains("NOT EXISTS"));
    assert_eq!(sql.params, vec![Value::Int(5)]);
}
#[test]
fn many_to_many_filters_use_a_correlated_join() {
    let q = Select::new("posts", &["id"])
        .where_(through().some(Field::<String>::new("categories", "name").equals("rust")))
        .compile()
        .unwrap();
    assert!(q.sql.contains("\"post_category\""), "{}", q.sql);
    assert!(q.sql.contains("EXISTS"));
    assert_eq!(q.params, vec![Value::String("rust".into())]);
    let q = Select::new("posts", &["id"])
        .where_(through().none(Field::<String>::new("categories", "name").equals("x")))
        .compile()
        .unwrap();
    assert!(q.sql.contains("NOT EXISTS"));
    let q = Select::new("posts", &["id"])
        .where_(through().every(Field::<String>::new("categories", "name").equals("x")))
        .compile()
        .unwrap();
    assert!(q.sql.contains("IS NOT TRUE"));
    assert!(Select::new("categories", &["id"])
        .where_(through().some(Field::<String>::new("categories", "name").equals("x")))
        .compile()
        .is_err());
    assert!(Select::new("posts", &["id"])
        .where_(through().some(Field::<i32>::new("posts", "id").equals(1)))
        .compile()
        .is_err());
}
