use std::collections::HashMap;

use wf_types::checkpoint::workflow::WorkflowExecutionStateSnapshot;
use wf_types::checkpoint::CheckpointContentConfig;

pub struct ContentFilter;

impl ContentFilter {
    pub fn new() -> Self {
        Self
    }

    pub fn should_include_state(&self, config: &CheckpointContentConfig) -> bool {
        config.include_state.unwrap_or(true)
    }

    pub fn should_include_history(&self, config: &CheckpointContentConfig) -> bool {
        config.include_history.unwrap_or(true)
    }

    pub fn should_include_statistics(&self, config: &CheckpointContentConfig) -> bool {
        config.include_statistics.unwrap_or(true)
    }
}

impl Default for ContentFilter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SizeBudget {
    max_snapshot_bytes: usize,
    max_message_count: usize,
}

impl SizeBudget {
    pub fn new(max_snapshot_bytes: usize, max_message_count: usize) -> Self {
        Self {
            max_snapshot_bytes,
            max_message_count,
        }
    }

    pub fn default_budget() -> Self {
        Self::new(1024 * 1024, 100)
    }

    pub fn truncate_messages<T: Clone>(&self, messages: Option<Vec<T>>) -> Option<Vec<T>> {
        messages.map(|msgs| {
            if msgs.len() > self.max_message_count {
                msgs[msgs.len() - self.max_message_count..].to_vec()
            } else {
                msgs
            }
        })
    }

    pub fn is_within_budget(&self, snapshot_bytes: usize) -> bool {
        snapshot_bytes <= self.max_snapshot_bytes
    }

    /// Progressive multi-step snapshot truncation aligned with the TS
    /// `applySnapshotSizeBudget`: messages (tail kept), then node results,
    /// then error/interruption/event records, then conversation state, then
    /// variable state. Each step is skipped when the snapshot is already
    /// within budget. Returns whether the snapshot is still over budget
    /// after all truncation steps.
    pub fn truncate_snapshot(&self, snapshot: &mut WorkflowExecutionStateSnapshot) -> bool {
        let limit = self.max_message_count.max(1);

        if let Some(messages) = &snapshot.messages {
            snapshot.messages = self.truncate_messages(Some(messages.clone()));
        }
        if !self.snapshot_over(snapshot) {
            return false;
        }

        if snapshot.node_results.is_some() {
            truncate_map(&mut snapshot.node_results, limit);
        }
        if !self.snapshot_over(snapshot) {
            return false;
        }

        truncate_records(&mut snapshot.error_records, limit);
        truncate_records(&mut snapshot.interruption_records, limit);
        truncate_records(&mut snapshot.event_records, limit);
        if !self.snapshot_over(snapshot) {
            return false;
        }

        if snapshot.conversation_state.is_some() {
            snapshot.conversation_state = None;
        }
        if !self.snapshot_over(snapshot) {
            return false;
        }

        snapshot.variable_state.variables.clear();
        self.snapshot_over(snapshot)
    }

    fn snapshot_over(&self, snapshot: &WorkflowExecutionStateSnapshot) -> bool {
        !self.is_within_budget(
            serde_json::to_vec(snapshot).map(|b| b.len()).unwrap_or(0),
        )
    }

    pub fn max_snapshot_bytes(&self) -> usize {
        self.max_snapshot_bytes
    }

    pub fn max_message_count(&self) -> usize {
        self.max_message_count
    }
}

impl Default for SizeBudget {
    fn default() -> Self {
        Self::default_budget()
    }
}

fn truncate_records(records: &mut Option<Vec<serde_json::Value>>, limit: usize) {
    if let Some(records) = records {
        if records.len() > limit {
            *records = records.split_off(records.len() - limit);
        }
    }
}

fn truncate_map(map: &mut Option<HashMap<String, serde_json::Value>>, limit: usize) -> bool {
    let Some(map) = map else {
        return false;
    };
    if map.len() <= limit {
        return false;
    }
    let mut entries: Vec<(String, serde_json::Value)> = map.drain().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.truncate(limit);
    *map = entries.into_iter().collect();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_filter_defaults() {
        let filter = ContentFilter::new();
        let config = CheckpointContentConfig {
            include_state: None,
            include_history: None,
            include_statistics: None,
            metadata: None,
            asynchronous: None,
        };
        assert!(filter.should_include_state(&config));
        assert!(filter.should_include_history(&config));
        assert!(filter.should_include_statistics(&config));
    }

    #[test]
    fn content_filter_respects_false() {
        let filter = ContentFilter::new();
        let config = CheckpointContentConfig {
            include_state: Some(false),
            include_history: Some(false),
            include_statistics: Some(false),
            metadata: None,
            asynchronous: None,
        };
        assert!(!filter.should_include_state(&config));
        assert!(!filter.should_include_history(&config));
        assert!(!filter.should_include_statistics(&config));
    }

    #[test]
    fn size_budget_truncates_messages() {
        let budget = SizeBudget::new(1024, 3);
        let messages: Vec<i32> = vec![1, 2, 3, 4, 5];
        let truncated = budget.truncate_messages(Some(messages)).unwrap();
        assert_eq!(truncated, vec![3, 4, 5]);
    }

    #[test]
    fn size_budget_keeps_small_vectors() {
        let budget = SizeBudget::new(1024, 10);
        let messages = vec![1, 2, 3];
        let result = budget.truncate_messages(Some(messages)).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn size_budget_checks_bytes() {
        let budget = SizeBudget::new(100, 10);
        assert!(budget.is_within_budget(50));
        assert!(budget.is_within_budget(100));
        assert!(!budget.is_within_budget(101));
    }

    #[test]
    fn truncate_snapshot_keeps_tail_messages() {
        use wf_types::checkpoint::CheckpointVariableState;
        use wf_types::message::{Message, MessageContentValue, MessageRole};

        let make_message = |id: &str| Message {
            id: id.to_string(),
            role: MessageRole::User,
            content: MessageContentValue::Text(format!("m-{}", id)),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };

        let mut snapshot = WorkflowExecutionStateSnapshot {
            execution_id: "exec-1".to_string(),
            status: "running".to_string(),
            current_node_id: None,
            node_results: None,
            variable_state: CheckpointVariableState {
                variables: HashMap::new(),
            },
            input: None,
            output: None,
            messages: Some(vec![make_message("1"), make_message("2"), make_message("3")]),
            fork_join_context: None,
            active_operations: None,
            conversation_state: None,
            trigger_states: None,
            error_records: None,
            interruption_records: None,
            event_records: None,
            hierarchy: None,
            execution_config: None,
            fork_join_aggregation_state: None,
            hook_execution_context: None,
            message_base_checkpoint_id: None,
            message_total_count: None,
        };

        // Budget generous enough for 2 messages but not 3.
        let budget = SizeBudget::new(512, 2);
        let still_over = budget.truncate_snapshot(&mut snapshot);
        assert!(!still_over);
        assert_eq!(snapshot.messages.as_ref().map(|m| m.len()), Some(2));
        assert_eq!(snapshot.messages.unwrap()[0].id, "2", "tail kept");
    }

    #[test]
    fn truncate_snapshot_progressively_drops_sections() {
        use wf_types::checkpoint::CheckpointVariableState;

        let mut snapshot = WorkflowExecutionStateSnapshot {
            execution_id: "exec-1".to_string(),
            status: "running".to_string(),
            current_node_id: None,
            node_results: Some(HashMap::from([(
                "node-1".to_string(),
                serde_json::json!({"payload": "x".repeat(2048)}),
            )])),
            variable_state: CheckpointVariableState {
                variables: HashMap::from([("k".to_string(), serde_json::json!("v"))]),
            },
            input: None,
            output: None,
            messages: None,
            fork_join_context: None,
            active_operations: None,
            conversation_state: Some(serde_json::json!({"big": "c".repeat(2048)})),
            trigger_states: None,
            error_records: Some(vec![serde_json::json!({"e": "x".repeat(1024)})]),
            interruption_records: None,
            event_records: None,
            hierarchy: None,
            execution_config: None,
            fork_join_aggregation_state: None,
            hook_execution_context: None,
            message_base_checkpoint_id: None,
            message_total_count: None,
        };

        let budget = SizeBudget::new(256, 10);
        let still_over = budget.truncate_snapshot(&mut snapshot);
        assert!(still_over, "small budget cannot be satisfied by truncation");
        assert!(snapshot.conversation_state.is_none(), "conversation dropped");
        assert!(
            snapshot.variable_state.variables.is_empty(),
            "variables dropped last"
        );
    }
}
