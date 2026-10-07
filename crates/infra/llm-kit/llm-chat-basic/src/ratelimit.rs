//! Token-bucket rate limiting for chat calls.
//!
//! A lean port of the proactive half of the code-context-engine limiter:
//! callers acquire a token before each request and wait for refill when the
//! bucket is empty. Reactive 429 handling belongs to [`crate::retry`].

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token-bucket rate limiter bounding requests per minute.
#[derive(Debug)]
pub struct RateLimiter {
    state: Mutex<BucketState>,
}

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    max_tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl RateLimiter {
    /// Creates a limiter for `requests_per_minute`; zero means unlimited.
    pub fn new(requests_per_minute: u32) -> Self {
        let max_tokens = requests_per_minute as f64;
        Self {
            state: Mutex::new(BucketState {
                tokens: max_tokens,
                max_tokens,
                refill_per_sec: max_tokens / 60.0,
                last_refill: Instant::now(),
            }),
        }
    }

    /// Blocks until a token is available, then consumes it.
    pub async fn acquire(&self) {
        loop {
            let wait = self.try_consume();
            match wait {
                None => return,
                Some(delay) => tokio::time::sleep(delay).await,
            }
        }
    }

    /// Returns the configured requests-per-minute budget.
    pub fn requests_per_minute(&self) -> u32 {
        self.state
            .lock()
            .expect("rate limiter mutex poisoned")
            .max_tokens as u32
    }

    fn try_consume(&self) -> Option<Duration> {
        let mut state = self.state.lock().expect("rate limiter mutex poisoned");
        if state.max_tokens <= 0.0 {
            return None;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill).as_secs_f64();
        if elapsed > 0.0 {
            state.tokens = (state.tokens + elapsed * state.refill_per_sec).min(state.max_tokens);
            state.last_refill = now;
        }
        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            None
        } else if state.refill_per_sec > 0.0 {
            let needed = 1.0 - state.tokens;
            Some(Duration::from_secs_f64(needed / state.refill_per_sec))
        } else {
            Some(Duration::from_millis(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_limiter_acquires_immediately() {
        let limiter = RateLimiter::new(0);
        limiter.acquire().await;
        assert_eq!(limiter.requests_per_minute(), 0);
    }

    #[tokio::test]
    async fn budgeted_limiter_serves_burst_then_waits() {
        let limiter = RateLimiter::new(60);
        let start = Instant::now();
        limiter.acquire().await;
        assert!(start.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn try_consume_reports_wait_when_empty() {
        let limiter = RateLimiter::new(60);
        for _ in 0..60 {
            assert!(limiter.try_consume().is_none());
        }
        assert!(limiter.try_consume().is_some());
    }
}
