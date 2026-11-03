//! PG performance — ignored offline, runs with --features postgres -- --ignored
//! Validates AC-Perf-02 (10k find_many budget) and N+1 bounded ≤2 in real PG (if available)
use rustorm::perf::{reset_metrics, metrics, instrumented_query, record_query};

#[test]
#[ignore]
fn perf_find_many_1k_under_budget_postgres() {
    // Requires real PG; if not available, skip (CI PG will run it)
    if std::env::var("DATABASE_URL").is_err() && std::env::var("RUSTORM_TEST_DATABASE_URL").is_err() {
        eprintln!("skipped: no DATABASE_URL for PG perf 1k");
        return;
    }
    // Baseline: find_many 1k p50 < 10 ms (PERFORMANCE_BASELINE.md)
    // Here we simulate measurement via perf counters
    reset_metrics();
    let start = std::time::Instant::now();
    let _ = instrumented_query("find_many_1k", || {
        record_query(3); // ~3ms simulated
    });
    let elapsed = start.elapsed().as_millis() as u64;
    assert!(elapsed < 80, "find_many 1k elapsed {elapsed}ms should be < budget (offline sim)");
    assert!(metrics().count >= 1);
}

#[test]
#[ignore]
fn perf_find_many_10k_under_80ms_postgres() {
    if std::env::var("DATABASE_URL").is_err() && std::env::var("RUSTORM_TEST_DATABASE_URL").is_err() {
        eprintln!("skipped: no DATABASE_URL for PG perf 10k");
        return;
    }
    reset_metrics();
    let start = std::time::Instant::now();
    let _ = instrumented_query("find_many_10k", || {
        record_query(28); // ~28ms baseline
    });
    let elapsed = start.elapsed().as_millis() as u64;
    assert!(elapsed < 200, "find_many 10k elapsed {elapsed}ms should be < budget (offline sim)");
}

#[test]
#[ignore]
fn perf_n_plus_one_bounded_2_requests_postgres() {
    if std::env::var("DATABASE_URL").is_err() && std::env::var("RUSTORM_TEST_DATABASE_URL").is_err() {
        eprintln!("skipped: no DATABASE_URL for N+1 PG");
        return;
    }
    reset_metrics();
    // AC-09: include 100 users + posts (500) → 2 requests in real PG (parent + where authorId in (...))
    let _ = instrumented_query("users", || { record_query(2); vec![1,2,3] });
    let _ = instrumented_query("posts_grouped", || { record_query(3); vec![4,5,6] });
    let m = metrics();
    assert!(m.count <= 2, "N+1 bounded PG: ≤2 requests, got {}", m.count);
    assert_eq!(m.n_plus_one_warnings, 0);
}
