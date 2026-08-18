//! Runtime tool visibility gate backed by workflow variables.
//!
//! The TOOL_VISIBILITY workflow node writes `__tool_blocked_<name>` markers
//! into the shared variable map; this store reads them at execution time.
//! Blocking is a runtime interception only — the model-visible schema stays
//! unchanged (KV-cache friendly), and the same store gates both direct calls
//! and `general` inner invocations (single execution-time interception
//! point, no second logic path).

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;

use crate::coordinator::tool::ToolVisibilityStore;

/// Variable prefix the TOOL_VISIBILITY node writes for blocked tools.
pub const BLOCKED_VARIABLE_PREFIX: &str = "__tool_blocked_";

/// Variable prefix marking formally activated (unblocked) tools, read by the
/// AGENT_LOOP node to seed the run's `ToolDiscoveryState`.
pub const ACTIVATED_VARIABLE_PREFIX: &str = "__tool_activated_";

/// A [`ToolVisibilityStore`] reading `__tool_blocked_<name>` workflow
/// variables.
#[derive(Clone)]
pub struct VariableBackedVisibilityStore {
    variables: Arc<DashMap<String, Value>>,
}

impl VariableBackedVisibilityStore {
    pub fn new(variables: Arc<DashMap<String, Value>>) -> Self {
        Self { variables }
    }

    /// Whether the workflow has blocked the given tool.
    pub fn is_blocked(&self, tool_name: &str) -> bool {
        self.variables
            .get(&format!("{}{}", BLOCKED_VARIABLE_PREFIX, tool_name))
            .map(|entry| entry.value() == &Value::Bool(true))
            .unwrap_or(false)
    }
}

#[async_trait]
impl ToolVisibilityStore for VariableBackedVisibilityStore {
    async fn is_tool_visible(&self, _execution_id: &str, tool_name: &str) -> bool {
        !self.is_blocked(tool_name)
    }
}

/// Collect the names of all formally activated tools from the workflow
/// variable map (`__tool_activated_<name>` markers), used to seed an agent
/// loop's `ToolDiscoveryState`.
pub fn collect_activated_tools(
    variables: &DashMap<String, Value>,
) -> std::collections::HashSet<String> {
    let mut activated = std::collections::HashSet::new();
    for entry in variables.iter() {
        if let Some(name) = entry.key().strip_prefix(ACTIVATED_VARIABLE_PREFIX) {
            if entry.value() == &Value::Bool(true) && !name.is_empty() {
                activated.insert(name.to_string());
            }
        }
    }
    activated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Arc<DashMap<String, Value>> {
        Arc::new(DashMap::new())
    }

    #[tokio::test]
    async fn unblocked_tools_are_visible() {
        let store = VariableBackedVisibilityStore::new(vars());
        assert!(store.is_tool_visible("exec-1", "read_file").await);
    }

    #[tokio::test]
    async fn blocked_tools_are_rejected() {
        let variables = vars();
        variables.insert(
            format!("{}{}", BLOCKED_VARIABLE_PREFIX, "shell"),
            Value::Bool(true),
        );
        let store = VariableBackedVisibilityStore::new(variables);
        assert!(!store.is_tool_visible("exec-1", "shell").await);
        assert!(store.is_tool_visible("exec-1", "read_file").await);
    }

    #[test]
    fn activated_markers_are_collected() {
        let variables = vars();
        variables.insert(
            format!("{}{}", ACTIVATED_VARIABLE_PREFIX, "write_file"),
            Value::Bool(true),
        );
        variables.insert(
            format!("{}{}", ACTIVATED_VARIABLE_PREFIX, "edit_file"),
            Value::Bool(true),
        );
        // Non-activated marker (false) is skipped; unrelated vars are ignored.
        variables.insert(
            format!("{}{}", ACTIVATED_VARIABLE_PREFIX, "other"),
            Value::Bool(false),
        );
        variables.insert("some_var".to_string(), Value::Bool(true));

        let activated = collect_activated_tools(&variables);
        assert_eq!(activated.len(), 2);
        assert!(activated.contains("write_file"));
        assert!(activated.contains("edit_file"));
        assert!(!activated.contains("other"));
    }
}
