//! Client-side circuit breaker keyed by service origin.
//!
//! Every JSON call funnels through [`post_json`](super::post_json), so a
//! service that stops answering trips one breaker no matter which channel
//! (fold, retrieval, navigation) noticed it first. Availability is a
//! transport property; which *subsystem* is healthy is a service-level
//! concern kept by the code-context adapters.
//!
//! A tripped breaker rejects calls without touching the network for a
//! cooldown, then lets one call through as the probe: success closes the
//! breaker, failure re-arms a longer cooldown. Recovery therefore needs no
//! background task and costs at most one blocked call per cooldown window.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// Consecutive failures before the breaker opens.
pub const FAILURE_THRESHOLD: u32 = 3;
/// First cooldown after opening; doubles per further failure.
pub const BASE_COOLDOWN_MS: u64 = 5_000;
/// Upper bound of the exponential cooldown.
pub const MAX_COOLDOWN_MS: u64 = 60_000;

/// Breaker position as observed by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BreakerPhase {
    /// Calls pass through.
    Closed,
    /// Calls are rejected until the cooldown lapses.
    Open,
    /// The cooldown lapsed; one call is being used as the probe.
    HalfOpen,
}

/// Point-in-time breaker report for status surfaces.
#[derive(Debug, Clone, Serialize)]
pub struct BreakerSnapshot {
    pub phase: BreakerPhase,
    pub consecutive_failures: u32,
    /// Remaining cooldown in milliseconds; zero unless open.
    pub retry_in_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct BreakerState {
    failures: u32,
    open_until: Option<Instant>,
    probing: bool,
}

#[derive(Debug)]
struct Inner {
    state: Mutex<BreakerState>,
}

/// One breaker guarding one origin.
#[derive(Debug)]
pub struct CircuitBreaker {
    origin: String,
    inner: Inner,
}

impl CircuitBreaker {
    fn new(origin: String) -> Self {
        Self {
            origin,
            inner: Inner {
                state: Mutex::new(BreakerState {
                    failures: 0,
                    open_until: None,
                    probing: false,
                }),
            },
        }
    }

    /// Origin this breaker guards (`scheme://authority`).
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Admit one call. A rejection carries the reason and never reaches the
    /// network; an admitted call after the cooldown becomes the probe.
    pub fn admit(&self) -> Result<(), String> {
        let mut state = self.lock();
        let Some(until) = state.open_until else {
            return Ok(());
        };
        let now = Instant::now();
        if now < until {
            let retry_in_ms = until.duration_since(now).as_millis() as u64;
            return Err(format!(
                "code-context service unavailable (circuit open, retry in {retry_in_ms} ms)"
            ));
        }
        if state.probing {
            return Err(
                "code-context service unavailable (circuit probe already in flight)".to_string(),
            );
        }
        state.probing = true;
        Ok(())
    }

    /// Close the breaker after a successful call or health probe.
    pub fn record_success(&self) {
        let mut state = self.lock();
        *state = BreakerState {
            failures: 0,
            open_until: None,
            probing: false,
        };
    }

    /// Count one observed failure; opens the breaker once the threshold is
    /// reached and extends the cooldown on repeated failures.
    pub fn record_failure(&self) {
        let mut state = self.lock();
        state.failures = state.failures.saturating_add(1);
        state.probing = false;
        if state.failures < FAILURE_THRESHOLD {
            return;
        }
        let steps = state.failures - FAILURE_THRESHOLD;
        let cooldown = BASE_COOLDOWN_MS
            .saturating_mul(1u64 << steps.min(4))
            .min(MAX_COOLDOWN_MS);
        state.open_until = Some(Instant::now() + Duration::from_millis(cooldown));
    }

    /// Reset the breaker for a freshly started process: a new process is a
    /// new availability fact, so the previous failure history is void.
    pub fn reset(&self) {
        self.record_success();
    }

    /// Snapshot for status reporting.
    pub fn snapshot(&self) -> BreakerSnapshot {
        let state = self.lock();
        let now = Instant::now();
        let retry_in_ms = state
            .open_until
            .map(|until| until.saturating_duration_since(now).as_millis() as u64)
            .unwrap_or(0);
        let phase = match state.open_until {
            None => BreakerPhase::Closed,
            Some(until) if now >= until => BreakerPhase::HalfOpen,
            Some(_) => BreakerPhase::Open,
        };
        BreakerSnapshot {
            phase,
            consecutive_failures: state.failures,
            retry_in_ms,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BreakerState> {
        match self.inner.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Origin (`scheme://authority`) of a service URL, used as the breaker key
/// so every endpoint of one service shares one breaker.
pub fn origin_of(url: &str) -> String {
    let trimmed = url.trim();
    match trimmed.split_once("://") {
        Some((scheme, rest)) => {
            let authority = rest.split('/').next().unwrap_or(rest);
            format!("{scheme}://{authority}")
        }
        None => trimmed.split('/').next().unwrap_or(trimmed).to_string(),
    }
}

fn registry() -> &'static Mutex<HashMap<String, Arc<CircuitBreaker>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<CircuitBreaker>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Breaker guarding the origin of `url`, created on first use.
pub fn breaker_for(url: &str) -> Arc<CircuitBreaker> {
    let origin = origin_of(url);
    let mut breakers = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    breakers
        .entry(origin.clone())
        .or_insert_with(|| Arc::new(CircuitBreaker::new(origin)))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_groups_endpoints_of_one_service() {
        assert_eq!(
            origin_of("http://127.0.0.1:9000/api/tools/fold/batch"),
            "http://127.0.0.1:9000"
        );
        assert_eq!(origin_of("http://127.0.0.1:9000"), "http://127.0.0.1:9000");
        assert_eq!(
            breaker_for("http://127.0.0.1:9000/api/search").origin(),
            breaker_for("http://127.0.0.1:9000/api/health").origin()
        );
    }

    #[test]
    fn breaker_opens_after_threshold_and_recovers_on_success() {
        let breaker = CircuitBreaker::new("http://breaker-test:1".into());
        assert!(breaker.admit().is_ok());
        for _ in 0..FAILURE_THRESHOLD {
            breaker.record_failure();
        }
        let snapshot = breaker.snapshot();
        assert_eq!(snapshot.phase, BreakerPhase::Open);
        assert!(snapshot.retry_in_ms > 0);
        let rejection = breaker.admit().expect_err("open breaker rejects calls");
        assert!(rejection.contains("circuit open"), "{rejection}");

        breaker.record_success();
        assert_eq!(breaker.snapshot().phase, BreakerPhase::Closed);
        assert!(breaker.admit().is_ok());
    }

    #[test]
    fn half_open_admits_one_probe() {
        let breaker = CircuitBreaker::new("http://breaker-halfopen:1".into());
        for _ in 0..FAILURE_THRESHOLD {
            breaker.record_failure();
        }
        {
            let mut state = breaker.lock();
            state.open_until = Some(Instant::now() - Duration::from_millis(1));
        }
        assert_eq!(breaker.snapshot().phase, BreakerPhase::HalfOpen);
        assert!(breaker.admit().is_ok());
        assert!(breaker.admit().is_err(), "second probe must be refused");
    }
}
