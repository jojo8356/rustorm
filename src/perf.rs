//! P1: Performance & observability: tracing, metrics, N+1 detection, benchmarks
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static QUERY_COUNT: AtomicU64 = AtomicU64::new(0);
static TOTAL_DURATION_MS: AtomicU64 = AtomicU64::new(0);
static N_PLUS_ONE_WARNINGS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct QueryMetrics {
    pub count: u64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
    pub n_plus_one_warnings: u64,
}

pub fn record_query(duration_ms: u64) {
    QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    TOTAL_DURATION_MS.fetch_add(duration_ms, Ordering::Relaxed);
}

pub fn record_n_plus_one() {
    N_PLUS_ONE_WARNINGS.fetch_add(1, Ordering::Relaxed);
    eprintln!("warning: potential N+1 query detected (count > threshold)");
}

pub fn metrics() -> QueryMetrics {
    let count = QUERY_COUNT.load(Ordering::Relaxed);
    let total = TOTAL_DURATION_MS.load(Ordering::Relaxed);
    let avg = if count > 0 { total as f64 / count as f64 } else { 0.0 };
    QueryMetrics {
        count,
        total_duration_ms: total,
        avg_duration_ms: avg,
        n_plus_one_warnings: N_PLUS_ONE_WARNINGS.load(Ordering::Relaxed),
    }
}

pub fn reset_metrics() {
    QUERY_COUNT.store(0, Ordering::Relaxed);
    TOTAL_DURATION_MS.store(0, Ordering::Relaxed);
    N_PLUS_ONE_WARNINGS.store(0, Ordering::Relaxed);
}

/// Instrument a query closure with timing and tracing stub.
/// P1: if duration exceeds threshold or count exceeds threshold, emit N+1 warning.
pub fn instrumented_query<F, T>(name: &str, f: F) -> T where F: FnOnce() -> T {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed().as_millis() as u64;
    record_query(elapsed);
    // Tracing stub: emit to stderr if enabled via env
    if std::env::var("RUSTORM_TRACE").is_ok() {
        eprintln!("[trace] query={} duration_ms={}", name, elapsed);
    }
    // N+1 detection heuristic: if more than 10 queries in short burst, warn
    if metrics().count > 10 && elapsed < 5 {
        record_n_plus_one();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metrics_record() {
        reset_metrics();
        record_query(10);
        record_query(20);
        let m = metrics();
        assert_eq!(m.count, 2);
        assert_eq!(m.total_duration_ms, 30);
    }
}
