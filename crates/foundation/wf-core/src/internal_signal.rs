//! Internal signal bus for type-safe communication between trigger actions,
//! workflow coordinators, and agent loop coordinators.
//!
//! Replaces the `__`-prefixed variable protocol (`__trigger_stop`,
//! `__trigger_pause`, `__skipped_{node_id}`, etc.) with a typed channel
//! that carries [`InternalSignal`] values. The old variable-protocol helpers
//! are retained for backward compatibility during the migration but new code
//! should prefer `InternalSignalBus`.
//!
//! # Architecture
//!
//! - [`InternalSignalBus`] wraps a `tokio::sync::broadcast` channel.
//! - Producers (trigger actions, message nodes) call `publish()`.
//! - Consumers (workflow coordinator loop, agent loop coordinator) call
//!   `subscribe()` and poll the returned `InternalSignalReceiver` at
//!   iteration/cycle boundaries.
//! - The channel is non-blocking and best-effort: a slow consumer that
//!   cannot keep up will be `Lagged` and miss signals. This is acceptable
//!   because the signals are idempotent and the consumer polls at
//!   well-defined checkpoints.
//! - Each subscriber gets an independent receiver; the `broadcast` channel
//!   delivers every signal to every active subscriber.

use tokio::sync::broadcast;
use wf_types::Id;

/// Default capacity of the internal signal channel.
const DEFAULT_CAPACITY: usize = 64;

/// Type-safe signals that replace the `__`-prefixed variable protocol.
///
/// Each variant carries a `source` (the execution id that published the
/// signal) so the consumer can filter signals from other executions (the
/// broadcast channel is shared across the process).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum InternalSignal {
    /// Stop the execution identified by `target_execution_id`.
    StopWorkflow {
        source: Id,
        target_execution_id: Id,
        reason: Option<String>,
    },
    /// Pause the execution.
    PauseWorkflow {
        source: Id,
        target_execution_id: Id,
        reason: Option<String>,
    },
    /// Resume the execution (clear the pause flag).
    ResumeWorkflow { source: Id, target_execution_id: Id },
    /// Skip a node in the execution.
    SkipNode {
        source: Id,
        target_execution_id: Id,
        node_id: String,
    },
    /// Result of a triggered sub-workflow.
    SubworkflowResult {
        source: Id,
        target_execution_id: Id,
        result: serde_json::Value,
    },
    /// Result of a triggered script.
    ScriptResult {
        source: Id,
        target_execution_id: Id,
        result: serde_json::Value,
    },
    /// Result of a triggered agent execution.
    AgentResult {
        source: Id,
        target_execution_id: Id,
        result: serde_json::Value,
    },
}

impl InternalSignal {
    /// The execution id that this signal targets.
    pub fn target_execution_id(&self) -> &Id {
        match self {
            InternalSignal::StopWorkflow {
                target_execution_id,
                ..
            } => target_execution_id,
            InternalSignal::PauseWorkflow {
                target_execution_id,
                ..
            } => target_execution_id,
            InternalSignal::ResumeWorkflow {
                target_execution_id,
                ..
            } => target_execution_id,
            InternalSignal::SkipNode {
                target_execution_id,
                ..
            } => target_execution_id,
            InternalSignal::SubworkflowResult {
                target_execution_id,
                ..
            } => target_execution_id,
            InternalSignal::ScriptResult {
                target_execution_id,
                ..
            } => target_execution_id,
            InternalSignal::AgentResult {
                target_execution_id,
                ..
            } => target_execution_id,
        }
    }

    /// Human-readable variant name for logging.
    pub fn variant_name(&self) -> &'static str {
        match self {
            InternalSignal::StopWorkflow { .. } => "StopWorkflow",
            InternalSignal::PauseWorkflow { .. } => "PauseWorkflow",
            InternalSignal::ResumeWorkflow { .. } => "ResumeWorkflow",
            InternalSignal::SkipNode { .. } => "SkipNode",
            InternalSignal::SubworkflowResult { .. } => "SubworkflowResult",
            InternalSignal::ScriptResult { .. } => "ScriptResult",
            InternalSignal::AgentResult { .. } => "AgentResult",
        }
    }
}

/// A receiver of [`InternalSignal`] values, obtained from
/// [`InternalSignalBus::subscribe()`].
pub struct InternalSignalReceiver {
    inner: broadcast::Receiver<InternalSignal>,
}

impl InternalSignalReceiver {
    /// Try to receive the next pending signal without blocking.
    ///
    /// Returns `None` when no signal is available, or when the subscriber
    /// has been lagged (signals are best-effort; the consumer should not
    /// depend on every signal when it cannot keep up).
    pub fn try_recv(&mut self) -> Option<InternalSignal> {
        match self.inner.try_recv() {
            Ok(signal) => Some(signal),
            Err(broadcast::error::TryRecvError::Empty) => None,
            Err(broadcast::error::TryRecvError::Closed) => None,
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                tracing::warn!("InternalSignalReceiver lagged by {} signals", n);
                None
            }
        }
    }

    /// Receive the next signal asynchronously.
    pub async fn recv(&mut self) -> Option<InternalSignal> {
        loop {
            match self.inner.recv().await {
                Ok(signal) => return Some(signal),
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("InternalSignalReceiver lagged by {} signals", n);
                    // Continue looping after lagging to get the next signal.
                    // The recursion is rewritten as a loop to avoid infinite future sizing.
                }
            }
        }
    }
}

/// A typed signal bus for internal workflow/agent signals.
///
/// Shared across the process via `Arc<InternalSignalBus>`. Producers
/// publish signals; consumers subscribe and poll at well-defined
/// checkpoints.
#[derive(Clone)]
pub struct InternalSignalBus {
    sender: broadcast::Sender<InternalSignal>,
}

impl InternalSignalBus {
    /// Create a new signal bus with the default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new signal bus with a specific channel capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Subscribe to receive all signals published on this bus.
    pub fn subscribe(&self) -> InternalSignalReceiver {
        InternalSignalReceiver {
            inner: self.sender.subscribe(),
        }
    }

    /// Publish a signal to all subscribers.
    ///
    /// Returns the number of subscribers that received the signal, or an
    /// error if there are no subscribers. Logs a warning on failure instead
    /// of silently dropping the signal.
    pub fn publish(&self, signal: InternalSignal) {
        let name = signal.variant_name();
        match self.sender.send(signal) {
            Ok(n) => {
                if n == 0 {
                    tracing::debug!(
                        signal_type = name,
                        "InternalSignal published with no subscribers"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    signal_type = name,
                    error = %e,
                    "InternalSignal publish failed (no receivers)"
                );
            }
        }
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for InternalSignalBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publish_and_receive() {
        let bus = InternalSignalBus::new();
        let mut rx = bus.subscribe();

        let signal = InternalSignal::StopWorkflow {
            source: Id::from("src-1"),
            target_execution_id: Id::from("target-1"),
            reason: None,
        };
        bus.publish(signal.clone());

        let received = rx.try_recv().expect("should receive signal");
        assert_eq!(received.target_execution_id(), &Id::from("target-1"));
    }

    #[test]
    fn test_filter_by_target() {
        let bus = InternalSignalBus::new();
        let mut rx = bus.subscribe();

        bus.publish(InternalSignal::StopWorkflow {
            source: Id::from("src-1"),
            target_execution_id: Id::from("exec-1"),
            reason: None,
        });
        bus.publish(InternalSignal::PauseWorkflow {
            source: Id::from("src-1"),
            target_execution_id: Id::from("exec-2"),
            reason: None,
        });

        // Drain all signals and check we can filter.
        let mut count = 0;
        while let Some(signal) = rx.try_recv() {
            if signal.target_execution_id() == &Id::from("exec-1") {
                count += 1;
            }
        }
        assert_eq!(count, 1, "only one signal targets exec-1");
    }

    #[test]
    fn test_multiple_subscribers() {
        let bus = InternalSignalBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(InternalSignal::ResumeWorkflow {
            source: Id::from("src-1"),
            target_execution_id: Id::from("exec-1"),
        });

        assert!(rx1.try_recv().is_some(), "rx1 should receive");
        assert!(rx2.try_recv().is_some(), "rx2 should receive");
    }

    #[test]
    fn test_empty_no_signal() {
        let bus = InternalSignalBus::new();
        let mut rx = bus.subscribe();
        assert!(rx.try_recv().is_none(), "no signal published yet");
    }
}
