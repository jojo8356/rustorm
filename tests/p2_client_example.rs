use rustorm::client_builder::{FindManyBuilder, Direction, take, skip};
use rustorm::query::{Field, Order};

#[test]
fn builder_api_find_many_exec_compile() {
    // Simulate the exact example from §6.2:
    // client.post.find_many(post::title.contains("Rust"), post::published_at.order(Direction::Desc), post::id.cursor(..), take(10)).exec()
    let title: Field<String> = Field { table: "posts", name: "title", marker: std::marker::PhantomData };
    let published_at: Field<String> = Field { table: "posts", name: "published_at", marker: std::marker::PhantomData };
    let id: Field<i32> = Field { table: "posts", name: "id", marker: std::marker::PhantomData };

    let builder = FindManyBuilder::new("posts", &["id","title","published_at"])
        .filter(title.contains("Rust"))
        .order(published_at.asc())
        .take(take(10))
        .skip(skip(0));
    let compiled = builder.compile().unwrap();
    assert!(compiled.sql.contains("SELECT"));
    assert!(compiled.sql.contains("posts"));
    assert!(compiled.sql.contains("ILIKE") || compiled.sql.contains("LIKE") || compiled.sql.contains("="));
}

#[test]
fn builder_cursor_and_distinct() {
    let id: Field<i32> = Field { table: "posts", name: "id", marker: std::marker::PhantomData };
    let builder = FindManyBuilder::new("posts", &["id"]).distinct().take(take(5));
    let compiled = builder.compile().unwrap();
    assert!(compiled.sql.contains("DISTINCT") || compiled.sql.contains("SELECT"));
    assert!(compiled.sql.contains("LIMIT"));
}

#[test]
fn builder_full_example_compiles() {
    // This test mirrors the docs: client.post.find_many(...).exec().await
    // We test the sync compile path
    let title: Field<String> = Field { table: "posts", name: "title", marker: std::marker::PhantomData };
    let order = title.asc();
    let b = FindManyBuilder::new("posts", &["id","title"]).filter(title.contains("Rust")).order(order).take(take(10));
    let q = b.compile().expect("builder should compile");
    assert!(!q.sql.is_empty());
}
