//! Execution context write-back registry (compression chain closure).
//!
//! The event-driven compression chain writes compressed message arrays back
//! into the *named message arrays of a live execution*. The workflow engine
//! owns such arrays in the execution variable map, so the registry maps an
//! execution id to a versioned write-back handle over its variable map and
//! the trigger listener (same crate) writes compressed arrays back through
//! it after a summary workflow completes.
//!
//! Agent sessions are not registered here: the agent engine consumes
//! `CONTEXT_COMPRESSION_COMPLETED` events itself (session self-consumption in
//! wf-agent), so this registry only ever knows workflow variable maps.
//!
//! Versioned write-back: a compression event carries the array version at
//! emission time; the write-back is discarded when the array moved past that
//! version (concurrent appends win over stale compression results).

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use wf_types::message::Message;

use crate::message_context;

/// Reason a versioned array write-back did not take effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteBackError {
    /// No write-back handle is registered for the execution id (the
    /// execution finished and was unregistered, or never registered).
    NotRegistered,
    /// The array moved past the expected version; the write is discarded
    /// (concurrent appends win over stale compression results).
    VersionMismatch { expected: u64, current: u64 },
    /// The execution is registered but does not own the named array.
    ContextNotFound,
}

impl std::fmt::Display for WriteBackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteBackError::NotRegistered => write!(f, "execution is not registered"),
            WriteBackError::VersionMismatch { expected, current } => write!(
                f,
                "array version mismatch (expected {}, current {})",
                expected, current
            ),
            WriteBackError::ContextNotFound => write!(f, "named array is not tracked"),
        }
    }
}

/// Versioned write-back target over the named message arrays of one
/// workflow execution.
#[async_trait]
pub trait ContextWriter: Send + Sync {
    /// Replace the message array of `context_id`, only when the array is
    /// still at `expected_version` (newer messages win otherwise).
    async fn write_context(
        &self,
        context_id: &str,
        messages: Vec<Message>,
        expected_version: u64,
    ) -> Result<(), WriteBackError>;
    /// Current version of the named array (None when not tracked).
    async fn current_version(&self, context_id: &str) -> Option<u64>;
}

/// Write-back handle over a live workflow execution's variable map.
struct VariableMapWriteBack {
    variables: Arc<DashMap<String, Value>>,
}

#[async_trait]
impl ContextWriter for VariableMapWriteBack {
    async fn write_context(
        &self,
        context_id: &str,
        messages: Vec<Message>,
        expected_version: u64,
    ) -> Result<(), WriteBackError> {
        if !message_context::has_context(&self.variables, context_id) {
            return Err(WriteBackError::ContextNotFound);
        }
        let current = message_context::array_version(&self.variables, context_id);
        if current != expected_version {
            return Err(WriteBackError::VersionMismatch {
                expected: expected_version,
                current,
            });
        }
        message_context::register_context(&self.variables, context_id, messages);
        Ok(())
    }

    async fn current_version(&self, context_id: &str) -> Option<u64> {
        if message_context::has_context(&self.variables, context_id) {
            Some(message_context::array_version(&self.variables, context_id))
        } else {
            None
        }
    }
}

/// Registry of live workflow executions' write-back targets, keyed by
/// execution id. Executions are registered at start and unregistered at end
/// by the execution entry points (wf-runtime assembly); the trigger listener
/// dispatches compression write-back through it.
#[derive(Default)]
pub struct ExecutionContextRegistry {
    entries: DashMap<String, Arc<dyn ContextWriter>>,
    /// Raw variable maps of live workflow executions, keyed by execution id.
    /// Used by the event-driven trigger listener to execute actions that
    /// mutate the emitting execution's variables (set_variable, stop/pause,
    /// skip_node).
    variable_maps: DashMap<String, Arc<DashMap<String, Value>>>,
}

impl ExecutionContextRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a workflow execution's variable map as its write-back target.
    pub fn register_workflow(
        &self,
        execution_id: impl Into<String>,
        variables: Arc<DashMap<String, Value>>,
    ) {
        let key = execution_id.into();
        self.variable_maps.insert(key.clone(), variables.clone());
        self.register(key, Arc::new(VariableMapWriteBack { variables }));
    }

    /// Register an execution's write-back target (generic form; used by
    /// tests with recording targets and by [`register_workflow`]).
    pub fn register(&self, execution_id: impl Into<String>, target: Arc<dyn ContextWriter>) {
        self.entries.insert(execution_id.into(), target);
    }

    pub fn unregister(&self, execution_id: &str) {
        self.entries.remove(execution_id);
        self.variable_maps.remove(execution_id);
    }

    pub fn registered(&self, execution_id: &str) -> bool {
        self.entries.contains_key(execution_id)
    }

    /// The live variable map of a registered workflow execution (`None` for
    /// unregistered executions and for non-workflow targets).
    pub fn variables_for(&self, execution_id: &str) -> Option<Arc<DashMap<String, Value>>> {
        self.variable_maps
            .get(execution_id)
            .map(|entry| entry.clone())
    }

    /// Versioned write-back into the execution's named array (see
    /// [`ContextWriter`] for the semantics).
    pub async fn write_context(
        &self,
        execution_id: &str,
        context_id: &str,
        messages: Vec<Message>,
        expected_version: u64,
    ) -> Result<(), WriteBackError> {
        match self.entries.get(execution_id) {
            Some(entry) => {
                entry
                    .write_context(context_id, messages, expected_version)
                    .await
            }
            None => Err(WriteBackError::NotRegistered),
        }
    }

    /// Current version of the execution's named array (None when the
    /// execution is not registered or does not own the array).
    pub async fn current_version(&self, execution_id: &str, context_id: &str) -> Option<u64> {
        let entry = self.entries.get(execution_id)?;
        entry.current_version(context_id).await
    }
}
