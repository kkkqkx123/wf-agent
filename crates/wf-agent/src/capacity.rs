use std::sync::RwLock;

use wf_common::gate::{ConcurrencyGate, GateError, GatePermit, GateStats};

/// Admission gate for agent loop executions: owns the capacity limit.
/// Decoupled from the registry so it is independently testable; the registry
/// keeps the permit lifecycle (permit stored in the entity, released when the
/// execution reaches a terminal state).
pub struct AgentCapacityGate {
    gate: RwLock<ConcurrencyGate>,
}

impl AgentCapacityGate {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            gate: RwLock::new(ConcurrencyGate::new(max_concurrent)),
        }
    }

    /// Reconfigure the concurrent-execution limit in place.
    pub fn set_max_concurrent(&self, max: usize) {
        *wf_common::lock::write_ok(self.gate.write()) = ConcurrencyGate::new(max);
    }

    pub fn max_concurrent(&self) -> usize {
        wf_common::lock::read_ok(self.gate.read()).max_concurrent()
    }

    pub fn available_permits(&self) -> usize {
        wf_common::lock::read_ok(self.gate.read()).available_permits()
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
    fn test_set_max_concurrent_rebuilds_gate() {
        let gate = AgentCapacityGate::new(1);
        gate.set_max_concurrent(3);
        assert_eq!(gate.max_concurrent(), 3);
        let _a = gate.try_acquire().expect("permit a");
        let _b = gate.try_acquire().expect("permit b");
        let _c = gate.try_acquire().expect("permit c");
        assert!(matches!(gate.try_acquire(), Err(GateError::Closed(_))));
    }
}
