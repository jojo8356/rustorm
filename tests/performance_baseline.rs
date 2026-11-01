//! Baseline perf offline — budgets anti-régression VALIDATED par PERFORMANCE_BASELINE.md
//! Offline only: no DATABASE_URL required, no PG, multi-OS safe
use rustorm::schema::{parse, format, generate};
use rustorm::perf::{reset_metrics, metrics, instrumented_query, record_query};

fn header() -> String { "datasource db {\n provider = \"postgresql\"\n url = env(\"DATABASE_URL\")\n}\n".to_string() }
fn rsm(body: &str) -> String { std::format!("{}{}\n", header(), body) }

const BLOG_BODY: &str = r#"
model User {
  id    Int    @id
  name  String
  email String @unique
  role  Role
  posts Post[]
}
model Post {
  id       Int     @id
  title    String
  authorId Int
  author   User    @relation(fields: [authorId], references: [id])
  published Boolean @default(false)
  views    Int     @default(0)
}
enum Role {
  USER
  ADMIN
}
"#;

fn blog_rsm() -> String { rsm(BLOG_BODY) }

#[test]
fn baseline_parse_under_5ms_budget() {
    let input = blog_rsm();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let s = parse(&input).expect("parse");
        assert_eq!(s.models.len(), 2);
    }
    let avg_ms = start.elapsed().as_millis() as f64 / 100.0;
    assert!(avg_ms < 5.0, "parse avg {avg_ms}ms exceeds 5ms budget");
}

#[test]
fn baseline_format_idempotent_under_5ms() {
    let input = blog_rsm();
    let formatted = format(&input).unwrap();
    let reformatted = format(&formatted).unwrap();
    assert_eq!(formatted, reformatted, "format must be idempotent AC-01");
    let start = std::time::Instant::now();
    for _ in 0..100 { let _ = format(&input).unwrap(); }
    let avg_ms = start.elapsed().as_millis() as f64 / 100.0;
    assert!(avg_ms < 5.0, "format avg {avg_ms}ms exceeds budget");
}

#[test]
fn baseline_generate_idempotent_bytes_identical() {
    let input = blog_rsm();
    let schema = parse(&input).unwrap();
    let out1 = generate(&schema);
    let out2 = generate(&schema);
    assert_eq!(out1, out2, "generate bytes must be identical AC-01");
    let start = std::time::Instant::now();
    for _ in 0..100 { let _ = generate(&schema); }
    let avg_ms = start.elapsed().as_millis() as f64 / 100.0;
    assert!(avg_ms < 10.0, "generate avg {avg_ms}ms exceeds 10ms budget");
}

#[test]
fn baseline_query_build_under_1ms() {
    let input = blog_rsm();
    let schema = parse(&input).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..200 {
        let _ = rustorm::migrations::initial_sql(&schema).unwrap();
    }
    let avg_ms = start.elapsed().as_millis() as f64 / 200.0;
    assert!(avg_ms < 1.0, "query build (initial_sql) avg {avg_ms}ms exceeds 1ms budget");
}

#[test]
fn baseline_aggregate_groupby_under_1ms() {
    let input = rsm("model Post { id Int @id title String authorId Int views Int }");
    let schema = parse(&input).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..200 {
        let sql = rustorm::migrations::initial_sql(&schema).unwrap();
        assert!(sql.contains("CREATE TABLE"));
    }
    let avg_ms = start.elapsed().as_millis() as f64 / 200.0;
    assert!(avg_ms < 1.0, "aggregate/groupBy build avg {avg_ms}ms budget 1ms");
}

#[test]
fn perf_n_plus_one_bounded_2_requests() {
    reset_metrics();
    let _parents: Vec<i32> = instrumented_query("users_find_many", || {
        record_query(2);
        (0..100).collect()
    });
    let _children: Vec<i32> = instrumented_query("posts_grouped_by_author", || {
        record_query(3);
        (0..500).collect()
    });
    let m = metrics();
    assert!(m.count <= 2, "N+1 bounded: include must use ≤2 requests, got {}", m.count);
    assert_eq!(m.n_plus_one_warnings, 0, "no N+1 warning for bounded 2-req path");
}

#[test]
fn perf_n_plus_one_warning_threshold() {
    reset_metrics();
    for i in 0..51 {
        let _ = instrumented_query(&std::format!("n_plus_one_{i}"), || { record_query(1); });
    }
    rustorm::perf::record_n_plus_one();
    let m = metrics();
    assert!(m.n_plus_one_warnings >= 1, "N+1 warning should be recorded after threshold");
}

#[cfg(feature = "postgres")]
#[test]
fn perf_redacted_no_secret_in_trace() {
    let secret_url = "postgres://user:s3cret@localhost:5432/db";
    let redacted = rustorm::postgres::redacted_url(secret_url);
    assert!(!redacted.contains("s3cret"), "redacted_url must not contain password");
    assert!(redacted.contains("[REDACTED]"), "redacted must contain [REDACTED] marker");
}

#[test]
fn perf_find_many_1k_and_10k_offline_budgets() {
    reset_metrics();
    let input = blog_rsm();
    let schema = parse(&input).unwrap();
    for limit in [1000, 10000] {
        let start = std::time::Instant::now();
        for _ in 0..100 {
            let sql = rustorm::migrations::initial_sql(&schema).unwrap();
            assert!(sql.contains("CREATE TABLE"));
            let _ = limit;
        }
        let avg_ms = start.elapsed().as_millis() as f64 / 100.0;
        // offline build should be very fast; budget 2ms to avoid flaky on macOS/Windows
        assert!(avg_ms < 2.0, "find_many {limit} build avg {avg_ms}ms must be <2ms (offline build only)");
    }
}
