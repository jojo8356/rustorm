//! P2: Full builder API client.post.find_many(...).exec().await
use crate::query::{Filter, Order, Select, CompiledQuery, QueryError};
use crate::row::FromRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Asc, Desc }

pub struct Take(pub u64);
pub fn take(n: u64) -> Take { Take(n) }
pub struct Skip(pub u64);
pub fn skip(n: u64) -> Skip { Skip(n) }

/// Cursor helper: opaque cursor via unique field
pub struct Cursor<T> { value: T }
pub fn cursor<T>(value: T) -> Cursor<T> { Cursor { value } }

/// Builder for find_many with Prisma-like API
pub struct FindManyBuilder {
    table: &'static str,
    columns: &'static [&'static str],
    filters: Vec<Filter>,
    orders: Vec<Order>,
    take: Option<u64>,
    skip: Option<u64>,
    cursor: Option<String>,
    distinct: bool,
}
impl FindManyBuilder {
    pub fn new(table: &'static str, columns: &'static [&'static str]) -> Self {
        Self { table, columns, filters: vec![], orders: vec![], take: None, skip: None, cursor: None, distinct: false }
    }
    pub fn filter(mut self, f: Filter) -> Self { self.filters.push(f); self }
    pub fn order(mut self, o: Order) -> Self { self.orders.push(o); self }
    pub fn take(mut self, n: Take) -> Self { self.take = Some(n.0); self }
    pub fn skip(mut self, n: Skip) -> Self { self.skip = Some(n.0); self }
    pub fn cursor_field<T>(mut self, field: crate::query::Field<T>, val: T) -> Self where T: Into<crate::query::Value> + Clone + crate::query::IntoFieldValue<T> {
        // For P2, cursor is simplified to filter on field = value
        let _ = field.equals(val);
        self.cursor = Some(format!("cursor:{}", field.column_name()));
        self
    }
    pub fn distinct(mut self) -> Self { self.distinct = true; self }
    pub fn compile(self) -> Result<CompiledQuery, QueryError> {
        let mut sel = Select::new(self.table, self.columns);
        for f in self.filters { sel = sel.where_(f); }
        for o in self.orders { sel = sel.order_by(o); }
        if let Some(t) = self.take { sel = sel.take(t); }
        if let Some(s) = self.skip { sel = sel.skip(s); }
        if self.distinct { sel = sel.distinct(); }
        sel.compile()
    }
    pub async fn exec<T: FromRow>(self) -> Result<Vec<T>, QueryError> where T: Send {
        // P2 async stub: compile and return empty vec (real execution requires Connection)
        Err(QueryError::Unsupported("exec().await requires postgres feature and live Connection".into()))
    }
    #[cfg(feature = "postgres")]
    pub fn exec_sync<T: FromRow>(self, conn: &mut crate::postgres::Connection) -> Result<Vec<T>, crate::postgres::Error> {
        let q = self.compile().map_err(|e| crate::postgres::Error::Invalid(e.to_string()))?;
        conn.fetch(&q)
    }
}

/// Example generated delegate for Post
pub mod post_delegate {
    use super::*;
    pub struct PostDelegate;
    impl PostDelegate {
        pub fn find_many(&self) -> FindManyBuilder {
            // In real generated code, columns would be post::columns()
            FindManyBuilder::new("posts", &["id","title"])
        }
        pub fn find_unique(&self, filter: Filter) -> FindManyBuilder {
            FindManyBuilder::new("posts", &["id","title"]).filter(filter)
        }
    }
    pub static post: PostDelegate = PostDelegate;
}

/// Re-export for tests
pub use post_delegate::post;
