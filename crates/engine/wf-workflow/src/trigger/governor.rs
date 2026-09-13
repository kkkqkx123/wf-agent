//! Fire-permit bookkeeping for trigger actions: the re-entrancy guard and the
//! per-scope `max_triggers` budget.
//!
//! Both are keyed by the matcher's dispatch key (`crate::trigger::matcher`'s
//! `match_key`), so one (execution,
//! template) pair never runs twice concurrently and never spends more than its
//! budget, while independent executions keep independent budgets.

use std::collections::HashMap;
use std::sync::Mutex;

use dashmap::DashMap;
use wf_common::lock::lock_ok;

/// Outcome of a request to fire one (scope, template) pair.
pub(crate) enum FirePermit {
    /// Cleared to run. The caller owns the claim and must
    /// [`TriggerGovernor::release`] the key once the action is over.
    Granted,
    /// A run of the same pair is already in flight.
    AlreadyRunning,
    /// The `max_triggers` budget of the key is spent.
    BudgetExhausted,
}

/// Re-entrancy guard plus firing budget, shared by every clone of the
/// listener so a claim taken during dispatch is visible to the action task
/// that releases it.
pub(crate) struct TriggerGovernor {
    /// Dispatch keys with a run in flight.
    in_flight: DashMap<String, ()>,
    /// Fire counts per dispatch key (only consulted when `max_triggers > 0`).
    trigger_counts: Mutex<HashMap<String, u32>>,
}

impl TriggerGovernor {
    pub(crate) fn new() -> Self {
        Self {
            in_flight: DashMap::new(),
            trigger_counts: Mutex::new(HashMap::new()),
        }
    }

    /// Claim `key` for a run and charge its budget. The re-entrancy claim is
    /// taken first so a duplicate arriving while the budget is being checked
    /// is reported as already running; a budget rejection gives the claim
    /// back. Zero budget means no capacity (use absent for unlimited, see
    /// `TriggerTemplate::max_triggers` validation); it never grants.
    pub(crate) fn request(&self, key: &str, max_triggers: Option<u32>) -> FirePermit {
        // Atomic claim: a present entry means a run for this pair is in flight.
        if self.in_flight.insert(key.to_string(), ()).is_some() {
            return FirePermit::AlreadyRunning;
        }
        let Some(max) = max_triggers else {
            return FirePermit::Granted;
        };
        if max == 0 {
            self.in_flight.remove(key);
            return FirePermit::BudgetExhausted;
        }
        let mut counts = lock_ok(self.trigger_counts.lock());
        let count = counts.entry(key.to_string()).or_insert(0);
        if *count >= max {
            drop(counts);
            self.in_flight.remove(key);
            return FirePermit::BudgetExhausted;
        }
        *count += 1;
        FirePermit::Granted
    }

    /// Give back the claim taken by [`Self::request`].
    pub(crate) fn release(&self, key: &str) {
        self.in_flight.remove(key);
    }

    #[cfg(test)]
    fn is_in_flight(&self, key: &str) -> bool {
        self.in_flight.contains_key(key)
    }

    #[cfg(test)]
    fn fire_count(&self, key: &str) -> u32 {
        let counts = lock_ok(self.trigger_counts.lock());
        counts.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(permit: &FirePermit) -> &'static str {
        match permit {
            FirePermit::Granted => "granted",
            FirePermit::AlreadyRunning => "already_running",
            FirePermit::BudgetExhausted => "budget_exhausted",
        }
    }

    #[test]
    fn claim_is_held_until_released() {
        let governor = TriggerGovernor::new();
        assert_eq!(outcome(&governor.request("exec:t", None)), "granted");
        assert!(governor.is_in_flight("exec:t"));
        assert_eq!(
            outcome(&governor.request("exec:t", None)),
            "already_running",
            "a second concurrent fire of the same pair is rejected"
        );
        governor.release("exec:t");
        assert!(!governor.is_in_flight("exec:t"));
        assert_eq!(
            outcome(&governor.request("exec:t", None)),
            "granted",
            "the pair is claimable again once the run finished"
        );
    }

    #[test]
    fn budget_is_charged_per_key_and_releases_the_claim() {
        let governor = TriggerGovernor::new();
        assert_eq!(outcome(&governor.request("exec:t", Some(1))), "granted");
        assert_eq!(governor.fire_count("exec:t"), 1);
        governor.release("exec:t");
        assert_eq!(
            outcome(&governor.request("exec:t", Some(1))),
            "budget_exhausted",
            "the budget survives the release"
        );
        assert!(
            !governor.is_in_flight("exec:t"),
            "a budget rejection must not leave a claim behind"
        );
    }

    #[test]
    fn zero_max_triggers_means_no_capacity() {
        let governor = TriggerGovernor::new();
        assert_eq!(
            outcome(&governor.request("exec:t", Some(0))),
            "budget_exhausted"
        );
        assert!(
            !governor.is_in_flight("exec:t"),
            "a zero-budget rejection must not leave a claim behind"
        );
        assert_eq!(governor.fire_count("exec:t"), 0);
    }

    #[test]
    fn independent_keys_never_share_state() {
        let governor = TriggerGovernor::new();
        assert_eq!(outcome(&governor.request("exec-a:t", Some(1))), "granted");
        governor.release("exec-a:t");
        assert_eq!(outcome(&governor.request("exec-b:t", Some(1))), "granted");
        assert_eq!(governor.fire_count("exec-a:t"), 1);
        assert_eq!(governor.fire_count("exec-b:t"), 1);
    }
}
