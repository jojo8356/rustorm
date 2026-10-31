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
pub mod nested_writes;
pub mod prisma_import;
pub mod perf;
pub mod client_builder;
pub mod extensions;
pub mod connectors_p2;

/// Build script helper for `build.rs` (P0 §6.1). Call in `build.rs` as `rustorm::generate_on_build("rustorm/schema.rsm")`.
/// Generates Rust client to `OUT_DIR/rustorm_generated.rs` and emits `cargo:rerun-if-changed`.
pub fn generate_on_build(schema_path: &str) {
    println!("cargo:rerun-if-changed={}", schema_path);
    let input = std::fs::read_to_string(schema_path).expect("schema file not found");
    let schema = crate::schema::parse(&input).expect("invalid schema");
    let output = crate::schema::generate(&schema);
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = std::path::Path::new(&out_dir).join("rustorm_generated.rs");
    std::fs::write(&dest, output).expect("write generated");
    let checksum = crate::checksum::sha256(input.as_bytes());
    println!("cargo:rustc-env=RUSTORM_SCHEMA_CHECKSUM={}", &checksum[..16]);
}
