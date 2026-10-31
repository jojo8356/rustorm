//! Rustorm's schema parser and typed, parameterized PostgreSQL query builder.
//!
//! The optional `postgres` feature provides synchronous PostgreSQL execution via libpq.
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod query;
pub mod relations;
pub mod row;
pub mod schema;

pub mod checksum;
pub mod migrations;
