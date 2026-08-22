use std::sync::RwLock;

use wf_common::gate::{AcquireStrategy, ConcurrencyGate, GateError, GatePermit, GateStats};

/// Admission gate for agent loop executions: owns the capacity limit and the
/// acquisition strategy. Decoupled from the registry so it is independently
/// testable; the registry keeps the permit lifecycle (permit stored in the
/// entity, released when the execution reaches a terminal state).
pub struct AgentCapacityGate {
    gate: RwLock<ConcurrencyGate>,
}

impl AgentCapacityGate {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            gate: RwLock::new(ConcurrencyGate::new(max_concurrent)),
        }
    }

    /// Builder-style acquisition strategy override (default `Reject`).
    pub fn with_acquire_strategy(self, strategy: AcquireStrategy) -> Self {
        self.set_acquire_strategy(strategy);
        self
    }

    /// Reconfigure the acquisition strategy in place.
    pub fn set_acquire_strategy(&self, strategy: AcquireStrategy) {
        let max = self.max_concurrent();
        *wf_common::lock::write_ok(self.gate.write()) =
            ConcurrencyGate::new(max).with_strategy(strategy);
    }

    /// Reconfigure the concurrent-execution limit in place; the configured
    /// strategy is preserved.
    pub fn set_max_concurrent(&self, max: usize) {
        let strategy = wf_common::lock::read_ok(self.gate.read()).strategy();
        *wf_common::lock::write_ok(self.gate.write()) =
            ConcurrencyGate::new(max).with_strategy(strategy);
    }

    pub fn max_concurrent(&self) -> usize {
        wf_common::lock::read_ok(self.gate.read()).max_concurrent()
    }

    pub fn available_permits(&self) -> usize {
        wf_common::lock::read_ok(self.gate.read()).available_permits()
    }

    pub fn strategy(&self) -> AcquireStrategy {
        wf_common::lock::read_ok(self.gate.read()).strategy()
    }

    pub fn stats(&self) -> GateStats {
        wf_common::lock::read_ok(self.gate.read()).stats()
    }

    /// Reserve one concurrency slot; fails when the limit is reached or the
    /// gate is closed.
    pub fn try_acquire(&self) -> Result<GatePermit, GateError> {
        wf_common::lock::read_ok(self.gate.read()).try_acquire()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_strategy_is_reject() {
        let gate = AgentCapacityGate::new(1);
        assert_eq!(gate.max_concurrent(), 1);
        let _first = gate.try_acquire().expect("first permit");
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
    }

    #[test]
    fn test_rejects_beyond_limit() {
        let gate = AgentCapacityGate::new(2);
        let _a = gate.try_acquire().expect("permit a");
        let _b = gate.try_acquire().expect("permit b");
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
        assert_eq!(gate.available_permits(), 0);
        drop(_a);
        assert_eq!(gate.available_permits(), 1);
    }

    #[test]
    fn test_set_max_concurrent_preserves_strategy() {
        let gate = AgentCapacityGate::new(1).with_acquire_strategy(AcquireStrategy::Wait);
        gate.set_max_concurrent(3);
        assert_eq!(gate.max_concurrent(), 3);
        // Still waiting-strategy: acquiring without contention succeeds and
        // the third permit is available.
        let _a = gate.try_acquire().expect("permit a");
        let _b = gate.try_acquire().expect("permit b");
        let _c = gate.try_acquire().expect("permit c");
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
    }

    #[test]
    fn test_set_acquire_strategy_in_place() {
        let gate = AgentCapacityGate::new(1);
        assert!(matches!(gate.try_acquire().and_then(|_| gate.try_acquire()), Err(GateError::Closed(_))));
        gate.set_acquire_strategy(AcquireStrategy::Wait);
        // Permit was released by the dropped first acquisition above; a
        // second acquisition is now possible under Wait too (same capacity).
        let _a = gate.try_acquire().expect("permit after strategy change");
        drop(_a);
        assert_eq!(gate.available_permits(), 1);
    }
}
