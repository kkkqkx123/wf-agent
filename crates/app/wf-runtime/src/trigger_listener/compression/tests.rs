use super::*;

#[test]
fn compression_policy_default_bounds_one_retry() {
    let policy = CompressionPolicy::default();
    assert_eq!(policy.max_retries, 1);
    assert!(policy.run_timeout_ms > 0);
}
