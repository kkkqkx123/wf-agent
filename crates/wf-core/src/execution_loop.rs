//! Common execution loop primitives for agent and workflow coordinators.
//!
//! Both [`AgentExecutionCoordinator`] and [`WorkflowCoordinator`] implement
//! similar iteration patterns: interruption gating (pause/resume/stop),
//! checkpoint integration, and metrics recording. This module provides
//! shared types and a lightweight trait to reduce duplication for these
//! cross-cutting concerns without forcing a single loop structure on the
//! two coordinators (which differ in iteration granularity — iterations
//! vs. nodes).
//!
//! [`AgentExecutionCoordinator`]: wf_agent::coordinator::execution::AgentExecutionCoordinator
//! [`WorkflowCoordinator`]: wf_workflow::coordinator::WorkflowCoordinator

use crate::interruption::state::InterruptionState;
use crate::interruption::InterruptionSignal;

/// Decision returned by interruption checks and iteration hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopDecision {
    /// Continue the loop normally.
    Continue,
    /// Pause the loop (wait for resume signal).
    Pause,
    /// Stop the loop immediately (terminal error / cancellation).
    Stop,
    /// The loop has completed all work.
    Complete,
}

/// Convert an optional interruption signal into a [`LoopDecision`].
impl From<Option<InterruptionSignal>> for LoopDecision {
    fn from(signal: Option<InterruptionSignal>) -> Self {
        match signal {
            Some(InterruptionSignal::Stop) => LoopDecision::Stop,
            Some(InterruptionSignal::Pause) => LoopDecision::Pause,
            Some(InterruptionSignal::Active) | None => LoopDecision::Continue,
        }
    }
}

/// Trait for entities that carry an interruption signal.
///
/// Implemented by [`AgentLoopEntity`] and [`WorkflowExecutionEntity`] so
/// that generic interruption helpers can be shared across coordinators.
///
/// [`AgentLoopEntity`]: wf_agent::entity::AgentLoopEntity
/// [`WorkflowExecutionEntity`]: wf_workflow::entity::WorkflowExecutionEntity
pub trait HasInterruption {
    /// Access the interruption signal.
    fn interruption(&self) -> &InterruptionState;
}

/// Check whether the entity has received a stop signal.
pub fn is_stopped(entity: &impl HasInterruption) -> bool {
    matches!(
        entity.interruption().check(),
        Some(InterruptionSignal::Stop)
    )
}

/// Check whether the entity has received a pause signal.
pub fn is_paused(entity: &impl HasInterruption) -> bool {
    matches!(
        entity.interruption().check(),
        Some(InterruptionSignal::Pause)
    )
}

/// Wait for a paused entity to resume, given an [`InterruptionState`] and
/// a way to wait for the state change. This is a standalone helper because
/// [`WorkflowExecutionEntity`] does not have a `wait_until_active` method;
/// only [`AgentLoopEntity`] does.
pub async fn wait_for_resume(interruption: &InterruptionState) {
    if matches!(interruption.check(), Some(InterruptionSignal::Pause)) {
        let mut rx = interruption.subscribe();
        loop {
            let signal = rx.borrow().clone();
            match signal {
                InterruptionSignal::Active | InterruptionSignal::Stop => return,
                InterruptionSignal::Pause => {
                    if rx.changed().await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// Check whether an interruption signal indicates a stop.
pub fn is_stop_signal(signal: Option<InterruptionSignal>) -> bool {
    matches!(signal, Some(InterruptionSignal::Stop))
}

/// Check whether an interruption signal indicates a pause.
pub fn is_pause_signal(signal: Option<InterruptionSignal>) -> bool {
    matches!(signal, Some(InterruptionSignal::Pause))
}