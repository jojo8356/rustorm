use rustorm::perf::{record_query, metrics, reset_metrics, instrumented_query};

#[test]
fn perf_metrics_count_and_duration() {
    reset_metrics();
    record_query(10);
    record_query(20);
    let m = metrics();
    assert_eq!(m.count, 2);
    assert_eq!(m.total_duration_ms, 30);
}

#[test]
fn instrumented_query_records() {
    reset_metrics();
    let result = instrumented_query("test_q", || 42);
    assert_eq!(result, 42);
    assert!(metrics().count >= 1);
}

#[test]
fn n_plus_one_detection_threshold() {
    reset_metrics();
    for _ in 0..11 { record_query(1); }
    let m = metrics();
    assert!(m.count > 10);
}

#[test]
fn tracing_env_stub() {
    // Ensure tracing stub doesn't panic when env not set
    let _ = instrumented_query("select", || ());
}
