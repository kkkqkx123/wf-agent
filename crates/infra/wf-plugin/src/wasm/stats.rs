use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::PluginResult;

/// Per-plugin guest call counters. All fields are monotonic within the
/// plugin lifetime; a reload creates a fresh instance with fresh counters.
pub struct WasmStats {
    calls: AtomicU64,
    ok: AtomicU64,
    failed: AtomicU64,
    timeouts: AtomicU64,
    fuel_consumed: AtomicU64,
    pool_hits: AtomicU64,
    pool_misses: AtomicU64,
    pool_drops: AtomicU64,
}

/// Point-in-time copy of [`WasmStats`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WasmStatsSnapshot {
    pub calls: u64,
    pub ok: u64,
    pub failed: u64,
    pub timeouts: u64,
    pub fuel_consumed: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub pool_drops: u64,
}

impl Default for WasmStats {
    fn default() -> Self {
        Self {
            calls: AtomicU64::new(0),
            ok: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            fuel_consumed: AtomicU64::new(0),
            pool_hits: AtomicU64::new(0),
            pool_misses: AtomicU64::new(0),
            pool_drops: AtomicU64::new(0),
        }
    }
}

impl WasmStats {
    /// Record one finished guest call. `fuel_used` is `None` for calls
    /// without a fuel budget (unmetered) or when the remaining fuel could
    /// not be read.
    pub fn record<T>(&self, result: &PluginResult<T>, fuel_used: Option<u64>) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match result {
            Ok(_) => {
                self.ok.fetch_add(1, Ordering::Relaxed);
            }
            Err(crate::error::PluginError::Timeout { .. }) => {
                self.timeouts.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                self.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Some(used) = fuel_used {
            self.fuel_consumed.fetch_add(used, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> WasmStatsSnapshot {
        let load = |v: &AtomicU64| v.load(Ordering::Relaxed);
        WasmStatsSnapshot {
            calls: load(&self.calls),
            ok: load(&self.ok),
            failed: load(&self.failed),
            timeouts: load(&self.timeouts),
            fuel_consumed: load(&self.fuel_consumed),
            pool_hits: load(&self.pool_hits),
            pool_misses: load(&self.pool_misses),
            pool_drops: load(&self.pool_drops),
        }
    }

    /// Record one session-pool acquire outcome.
    pub fn record_pool_hit(&self) {
        self.pool_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one fresh session build (pool empty, disabled, or guest
    /// without heap-reset support).
    pub fn record_pool_miss(&self) {
        self.pool_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one session discarded instead of returned to the pool.
    pub fn record_pool_drop(&self) {
        self.pool_drops.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PluginError;

    #[test]
    fn counters_split_by_outcome() {
        let stats = WasmStats::default();
        stats.record::<()>(&Ok(()), Some(10));
        stats.record::<()>(&Ok(()), None);
        stats.record::<()>(
            &Err(PluginError::Timeout {
                plugin_id: "p".into(),
            }),
            Some(5),
        );
        stats.record::<()>(&Err(PluginError::WasmError("x".into())), None);
        stats.record_pool_hit();
        stats.record_pool_miss();
        stats.record_pool_drop();
        let snap = stats.snapshot();
        assert_eq!(
            snap,
            WasmStatsSnapshot {
                calls: 4,
                ok: 2,
                failed: 1,
                timeouts: 1,
                fuel_consumed: 15,
                pool_hits: 1,
                pool_misses: 1,
                pool_drops: 1,
            }
        );
    }
}
