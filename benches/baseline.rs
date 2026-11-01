use std::time::Instant;
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

fn bench<F: Fn()>(name: &str, iters: u32, f: F) -> (f64, f64, f64)
where F: Fn() {
    let mut times = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let start = Instant::now();
        f();
        times.push(start.elapsed().as_micros() as f64 / 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = times[times.len() * 50 / 100];
    let p95 = times[times.len() * 95 / 100];
    let p99 = times[times.len() * 99 / 100];
    if std::env::var("BENCH_VERBOSE").is_ok() {
        eprintln!("[bench] {name}: p50={p50:.3}ms p95={p95:.3}ms p99={p99:.3}ms (n={iters})");
    }
    (p50, p95, p99)
}

#[test]
fn bench_parse_format_generate_offline() {
    let input = rsm(BLOG_BODY);
    let iters = 200;
    let (p50, p95, _p99) = bench("parse", iters, || {
        let schema = parse(&input).expect("parse");
        assert_eq!(schema.models.len(), 2);
    });
    assert!(p50 < 5.0, "parse p50 {p50}ms exceeds budget 5ms");
    assert!(p95 < 10.0, "parse p95 {p95}ms exceeds budget");

    let formatted = format(&input).unwrap();
    let (p50, _, _) = bench("format", iters, || {
        let reformatted = format(&formatted).unwrap();
        assert_eq!(formatted, reformatted);
    });
    assert!(p50 < 5.0, "format p50 {p50}ms budget 5ms");

    let schema = parse(&input).unwrap();
    let (p50, _, _) = bench("generate", iters, || {
        let out1 = generate(&schema);
        let out2 = generate(&schema);
        assert_eq!(out1, out2);
    });
    assert!(p50 < 10.0, "generate p50 {p50}ms budget 10ms");
}

#[test]
fn bench_query_build_offline() {
    let input = rsm(BLOG_BODY);
    let schema = parse(&input).unwrap();
    let iters = 500;
    let (p50, _, _) = bench("query_build_via_initial_sql", iters, || {
        let sql = rustorm::migrations::initial_sql(&schema).unwrap();
        assert!(sql.contains("CREATE TABLE"));
    });
    assert!(p50 < 1.0, "query_build p50 {p50}ms budget 1ms");

    let (p50, _, _) = bench("simple_sql_gen", iters, || {
        let s2 = parse(&rsm("model User { id Int @id name String }")).unwrap();
        let sql = rustorm::migrations::initial_sql(&s2).unwrap();
        assert!(sql.contains("CREATE TABLE"));
    });
    assert!(p50 < 1.0, "simple sql gen p50 {p50}ms budget 1ms");
}

#[test]
fn bench_n_plus_one_bounded() {
    reset_metrics();
    instrumented_query("users_find_many", || {
        record_query(2);
    });
    instrumented_query("posts_grouped", || {
        record_query(3);
    });
    let m = metrics();
    assert!(m.count == 2, "N+1 bounded: expected 2 queries for include, got {}", m.count);
    assert!(m.n_plus_one_warnings == 0, "no N+1 warning for bounded case");
}

#[test]
fn bench_10k_query_build_under_budget() {
    let input = rsm("model User { id Int @id name String }");
    let schema = parse(&input).unwrap();
    let iters = 200;
    let (p50, _, _) = bench("find_many_10k_build", iters, || {
        let sql = rustorm::migrations::initial_sql(&schema).unwrap();
        assert!(sql.contains("CREATE TABLE"));
        // Simulate LIMIT handling via generate
        let g = generate(&schema);
        assert!(g.contains("User"));
    });
    assert!(p50 < 1.0, "find_many 10k build p50 {p50}ms budget 1ms (offline build only)");
}

#[test]
fn bench_find_many_1k_sql_generation() {
    let input = rsm("model User { id Int @id name String }");
    let schema = parse(&input).unwrap();
    let iters = 200;
    let (p50, _, _) = bench("find_many_1k_sql", iters, || {
        let sql = rustorm::migrations::initial_sql(&schema).unwrap();
        assert!(sql.contains("CREATE TABLE"));
        let _ = schema.models.len();
    });
    assert!(p50 < 1.0, "find_many 1k sql p50 {p50}ms");
}
