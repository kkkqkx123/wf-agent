use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};

pub use wf_types::Timestamp;

pub fn now() -> Timestamp {
    Utc::now().timestamp_millis()
}

/// Wall-clock milliseconds since the Unix epoch. Centralized here so
/// servers, sandboxes and other callers share one epoch-ms source.
pub fn epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Poll `cond` every `interval` until it returns `true` or `deadline`
/// elapses. The condition is checked once immediately, then after each
/// sleep. Returns `true` when the condition was met, `false` on timeout.
///
/// Shared by production settle-polls (compression, event wait) and test
/// `wait_until` helpers so poll intervals and timeout boundaries come from
/// one place instead of ad-hoc sleep loops.
pub async fn poll_until<F, Fut>(interval: Duration, deadline: Duration, mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = std::time::Instant::now();
    loop {
        if cond().await {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(interval).await;
    }
}

pub fn datetime_from_timestamp(ts: Timestamp) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ts).unwrap_or(DateTime::UNIX_EPOCH)
}

pub fn timestamp_to_iso(ts: Timestamp) -> String {
    datetime_from_timestamp(ts).to_rfc3339()
}
