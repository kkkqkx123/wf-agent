//! Reserved trigger protocol variables and internal signal helpers.
//!
//! Trigger actions and message nodes communicate with the running execution
//! through two mechanisms:
//!
//! 1. **InternalSignalBus** — the typed channel for control signals
//!    (stop/pause/resume/skip) and async results. This is the only channel
//!    for control; the coordinators subscribe and react at loop boundaries.
//!
//! 2. **Reserved variables** (`__`-prefixed) in the workflow variable map
//!    for the remaining user-visible contracts: node idempotency markers
//!    (`__completed_{node_id}`, consumed by single-execution node handlers)
//!    and result slots (`__trigger_*_result`, the default `result_variable`
//!    targets). Keep them centralized here so the contract stays visible
//!    and user variables cannot collide with them.

use serde_json::Value;
use wf_core::internal_signal::{InternalSignal, InternalSignalBus};
use wf_types::Id;

// ── Reserved user-visible variables ───────────────────────────────────────

/// Idempotency marker for single-execution node handlers (START / END /
/// message nodes): `__completed_{node_id}`.
pub fn completed_marker(node_id: &str) -> String {
    format!("__completed_{}", node_id)
}
/// Result slot for triggered sub-workflows
/// (`TriggerAction::ExecuteTriggeredSubworkflow`).
pub const SUBWORKFLOW_RESULT: &str = "__trigger_subworkflow_result";
/// Result slot for triggered scripts (`TriggerAction::ExecuteScript`).
pub const SCRIPT_RESULT: &str = "__trigger_script_result";
/// Default result slot for triggered agent executions when no
/// `result_variable` is configured (wf-runtime `AgentTriggerRunner`).
pub const AGENT_RESULT: &str = "__trigger_agent_result";

// ── Internal signal helpers ───────────────────────────────────────────────

/// Publish a stop signal via the `InternalSignalBus` (typed channel).
///
/// This is the only mechanism for stop; the coordinator loop subscribes to
/// the bus and reacts to the signal at node boundaries.
pub fn publish_stop_signal(
    bus: &InternalSignalBus,
    source: Id,
    target_execution_id: Id,
    reason: Option<String>,
) {
    bus.publish(InternalSignal::StopWorkflow {
        source,
        target_execution_id,
        reason,
    });
}

/// Publish a pause signal via the `InternalSignalBus`.
pub fn publish_pause_signal(
    bus: &InternalSignalBus,
    source: Id,
    target_execution_id: Id,
    reason: Option<String>,
) {
    bus.publish(InternalSignal::PauseWorkflow {
        source,
        target_execution_id,
        reason,
    });
}

/// Publish a resume signal via the `InternalSignalBus`.
pub fn publish_resume_signal(bus: &InternalSignalBus, source: Id, target_execution_id: Id) {
    bus.publish(InternalSignal::ResumeWorkflow {
        source,
        target_execution_id,
    });
}

/// Publish a skip-node signal via the `InternalSignalBus`.
pub fn publish_skip_signal(
    bus: &InternalSignalBus,
    source: Id,
    target_execution_id: Id,
    node_id: String,
) {
    bus.publish(InternalSignal::SkipNode {
        source,
        target_execution_id,
        node_id,
    });
}

/// Publish a sub-workflow result signal via the `InternalSignalBus`.
pub fn publish_subworkflow_result(
    bus: &InternalSignalBus,
    source: Id,
    target_execution_id: Id,
    result: Value,
) {
    bus.publish(InternalSignal::SubworkflowResult {
        source,
        target_execution_id,
        result,
    });
}

/// Publish a script result signal via the `InternalSignalBus`.
pub fn publish_script_result(
    bus: &InternalSignalBus,
    source: Id,
    target_execution_id: Id,
    result: Value,
) {
    bus.publish(InternalSignal::ScriptResult {
        source,
        target_execution_id,
        result,
    });
}

/// Publish an agent result signal via the `InternalSignalBus`.
pub fn publish_agent_result(
    bus: &InternalSignalBus,
    source: Id,
    target_execution_id: Id,
    result: Value,
) {
    bus.publish(InternalSignal::AgentResult {
        source,
        target_execution_id,
        result,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Reserved variable helpers tests ───────────────────────────────

    #[test]
    fn markers_are_namespaced() {
        assert_eq!(completed_marker("n1"), "__completed_n1");
        assert_ne!(completed_marker("n1"), completed_marker("n2"));
    }

    // ── Internal signal bus tests ─────────────────────────────────────

    #[test]
    fn publish_stop_signal_roundtrip() {
        let bus = InternalSignalBus::new();
        let mut rx = bus.subscribe();

        publish_stop_signal(
            &bus,
            Id::from("src"),
            Id::from("target"),
            Some("reason".into()),
        );

        let signal = rx.try_recv().expect("signal must be delivered");
        match signal {
            InternalSignal::StopWorkflow {
                source,
                target_execution_id,
                reason,
            } => {
                assert_eq!(source, Id::from("src"));
                assert_eq!(target_execution_id, Id::from("target"));
                assert_eq!(reason, Some("reason".to_string()));
            }
            _ => panic!("unexpected signal variant"),
        }
    }

    #[test]
    fn publish_pause_and_resume_signals() {
        let bus = InternalSignalBus::new();
        let mut rx = bus.subscribe();

        publish_pause_signal(&bus, Id::from("src"), Id::from("target"), None);
        publish_resume_signal(&bus, Id::from("src"), Id::from("target"));

        let s1 = rx.try_recv().expect("pause signal");
        assert!(matches!(s1, InternalSignal::PauseWorkflow { .. }));

        let s2 = rx.try_recv().expect("resume signal");
        assert!(matches!(s2, InternalSignal::ResumeWorkflow { .. }));
    }

    #[test]
    fn publish_skip_signal() {
        let bus = InternalSignalBus::new();
        let mut rx = bus.subscribe();

        super::publish_skip_signal(&bus, Id::from("src"), Id::from("target"), "node-42".into());

        let signal = rx.try_recv().expect("skip signal");
        match signal {
            InternalSignal::SkipNode { node_id, .. } => {
                assert_eq!(node_id, "node-42");
            }
            _ => panic!("unexpected signal variant"),
        }
    }
}
