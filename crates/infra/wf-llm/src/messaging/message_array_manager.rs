use std::sync::Mutex;

use wf_types::message::BatchSnapshot;
use wf_types::message::{
    AppendMessageOperation, ClearMessageOperation, FilterMessageOperation, InsertMessageOperation,
    Message, MessageOperationConfig, MessageOperationResult, MessageOperationStats,
    ReplaceMessageOperation, RollbackMessageOperation, TruncateMessageOperation,
};

#[derive(Debug)]
struct InternalState {
    messages: Vec<Message>,
    batch_snapshots: Vec<BatchSnapshot>,
    current_batch_index: u32,
}

pub struct MessageArrayManager {
    state: Mutex<InternalState>,
}

impl MessageArrayManager {
    pub fn new(initial_messages: Vec<Message>) -> Self {
        Self {
            state: Mutex::new(InternalState {
                messages: initial_messages,
                batch_snapshots: Vec::new(),
                current_batch_index: 0,
            }),
        }
    }

    pub fn execute(&self, operation: MessageOperationConfig) -> MessageOperationResult {
        match operation {
            MessageOperationConfig::Append(op) => self.execute_append(op),
            MessageOperationConfig::Insert(op) => self.execute_insert(op),
            MessageOperationConfig::Replace(op) => self.execute_replace(op),
            MessageOperationConfig::Truncate(op) => self.execute_truncate(op),
            MessageOperationConfig::Clear(op) => self.execute_clear(op),
            MessageOperationConfig::Filter(op) => self.execute_filter(op),
            MessageOperationConfig::Rollback(op) => self.execute_rollback(op),
            MessageOperationConfig::BatchManagement(_) => MessageOperationResult {
                messages: wf_common::lock::lock_ok(self.state.lock()).messages.clone(),
                affected_batch_index: None,
                stats: MessageOperationStats {
                    added: 0,
                    removed: 0,
                    modified: 0,
                    total_after: 0,
                },
            },
        }
    }

    pub fn current_messages(&self) -> Vec<Message> {
        wf_common::lock::lock_ok(self.state.lock()).messages.clone()
    }

    pub fn get_batch_snapshot(&self, batch_index: u32) -> Option<BatchSnapshot> {
        let state = wf_common::lock::lock_ok(self.state.lock());
        state.batch_snapshots.get(batch_index as usize).cloned()
    }

    pub fn rollback_to(&self, batch_index: u32) -> MessageOperationResult {
        self.execute_rollback(RollbackMessageOperation {
            target_batch: batch_index,
        })
    }

    fn execute_append(&self, op: AppendMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());
        let original_count = state.messages.len() as u32;
        state.messages.extend(op.messages.clone());
        let new_count = state.messages.len() as u32;

        MessageOperationResult {
            messages: state.messages.clone(),
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: new_count - original_count,
                removed: 0,
                modified: 0,
                total_after: new_count,
            },
        }
    }

    fn execute_insert(&self, op: InsertMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());
        let original_count = state.messages.len() as u32;

        if op.index as usize > state.messages.len() {
            panic!(
                "Invalid insert index: {} (len: {})",
                op.index,
                state.messages.len()
            );
        }

        let snapshot = self.create_snapshot(&state);
        let idx = op.index as usize;
        for (i, msg) in op.messages.iter().enumerate() {
            state.messages.insert(idx + i, msg.clone());
        }

        state.batch_snapshots.push(snapshot);
        state.current_batch_index += 1;
        let new_count = state.messages.len() as u32;

        MessageOperationResult {
            messages: state.messages.clone(),
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: new_count - original_count,
                removed: 0,
                modified: 0,
                total_after: new_count,
            },
        }
    }

    fn execute_replace(&self, op: ReplaceMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());

        if op.index as usize >= state.messages.len() {
            panic!(
                "Invalid replace index: {} (len: {})",
                op.index,
                state.messages.len()
            );
        }

        let snapshot = self.create_snapshot(&state);
        state.messages[op.index as usize] = op.message.clone();

        state.batch_snapshots.push(snapshot);
        state.current_batch_index += 1;

        MessageOperationResult {
            messages: state.messages.clone(),
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: 0,
                removed: 0,
                modified: 1,
                total_after: state.messages.len() as u32,
            },
        }
    }

    fn execute_truncate(&self, op: TruncateMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());
        let original_count = state.messages.len() as u32;

        let snapshot = self.create_snapshot(&state);

        let new_messages = if op.from_end.unwrap_or(false) {
            let keep = op.keep_count as usize;
            if keep >= state.messages.len() {
                state.messages.clone()
            } else {
                state.messages[state.messages.len() - keep..].to_vec()
            }
        } else {
            state.messages[..op.keep_count as usize].to_vec()
        };

        state.messages = new_messages;
        state.batch_snapshots.push(snapshot);
        state.current_batch_index += 1;
        let new_count = state.messages.len() as u32;

        MessageOperationResult {
            messages: state.messages.clone(),
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: 0,
                removed: original_count - new_count,
                modified: 0,
                total_after: new_count,
            },
        }
    }

    fn execute_clear(&self, _op: ClearMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());
        let original_count = state.messages.len() as u32;

        let snapshot = BatchSnapshot {
            id: format!("batch_{}", state.current_batch_index),
            messages: Vec::new(),
            timestamp: wf_common::time::now(),
        };

        state.messages.clear();
        state.batch_snapshots.push(snapshot);
        state.current_batch_index += 1;

        MessageOperationResult {
            messages: vec![],
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: 0,
                removed: original_count,
                modified: 0,
                total_after: 0,
            },
        }
    }

    fn execute_filter(&self, op: FilterMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());
        let original_count = state.messages.len() as u32;

        let snapshot = self.create_snapshot(&state);

        if let Some(role) = &op.role {
            if op.exclude.unwrap_or(false) {
                state.messages.retain(|m| m.role != *role);
            } else {
                state.messages.retain(|m| m.role == *role);
            }
        }

        if let Some(custom) = &op.custom_filter {
            state.messages.retain(|m| match &m.content {
                wf_types::message::MessageContentValue::Text(s) => s.contains(custom),
                wf_types::message::MessageContentValue::Rich(contents) => {
                    contents.iter().any(|c| matches!(c, wf_types::message::MessageContent::Text { text } if text.contains(custom)))
                }
            });
        }

        state.batch_snapshots.push(snapshot);
        state.current_batch_index += 1;
        let new_count = state.messages.len() as u32;

        MessageOperationResult {
            messages: state.messages.clone(),
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: 0,
                removed: original_count - new_count,
                modified: 0,
                total_after: new_count,
            },
        }
    }

    fn execute_rollback(&self, op: RollbackMessageOperation) -> MessageOperationResult {
        let mut state = wf_common::lock::lock_ok(self.state.lock());

        if op.target_batch > state.current_batch_index {
            panic!(
                "Invalid rollback target: {} (current: {})",
                op.target_batch, state.current_batch_index
            );
        }

        if op.target_batch as usize >= state.batch_snapshots.len() {
            state.messages = vec![];
            state.batch_snapshots = vec![];
            state.current_batch_index = 0;
        } else {
            let snapshot = &state.batch_snapshots[op.target_batch as usize];
            state.messages = snapshot.messages.clone();
            state.batch_snapshots.truncate(op.target_batch as usize);
            state.current_batch_index = op.target_batch;
        }

        let total = state.messages.len() as u32;
        MessageOperationResult {
            messages: state.messages.clone(),
            affected_batch_index: Some(state.current_batch_index),
            stats: MessageOperationStats {
                added: 0,
                removed: 0,
                modified: 0,
                total_after: total,
            },
        }
    }

    fn create_snapshot(&self, state: &InternalState) -> BatchSnapshot {
        BatchSnapshot {
            id: format!("batch_{}", state.current_batch_index),
            messages: state.messages.clone(),
            timestamp: wf_common::time::now(),
        }
    }
}

impl Default for MessageArrayManager {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn test_append() {
        let mgr = MessageArrayManager::new(vec![msg(MessageRole::System, "sys")]);
        let result = mgr.execute(MessageOperationConfig::Append(AppendMessageOperation {
            messages: vec![msg(MessageRole::User, "hello")],
            batch_index: None,
        }));

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.stats.added, 1);
    }

    #[test]
    fn test_insert() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Insert(InsertMessageOperation {
            messages: vec![msg(MessageRole::Assistant, "a1")],
            index: 1,
        }));

        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_replace() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "old"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Replace(ReplaceMessageOperation {
            index: 1,
            message: msg(MessageRole::User, "new"),
        }));

        assert_eq!(result.stats.modified, 1);
        match &result.messages[1].content {
            MessageContentValue::Text(s) => assert_eq!(s, "new"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_truncate() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::User, "q2"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Truncate(TruncateMessageOperation {
            keep_count: 2,
            from_end: None,
        }));

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.stats.removed, 2);
    }

    #[test]
    fn test_truncate_from_end() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::User, "q2"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Truncate(TruncateMessageOperation {
            keep_count: 2,
            from_end: Some(true),
        }));

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, MessageRole::Assistant);
    }

    #[test]
    fn test_clear() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Clear(ClearMessageOperation {
            batch_index: None,
        }));

        assert!(result.messages.is_empty());
        assert_eq!(result.stats.removed, 2);
    }

    #[test]
    fn test_filter_by_role() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::User, "q2"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Filter(FilterMessageOperation {
            role: Some(MessageRole::User),
            exclude: None,
            custom_filter: None,
        }));

        assert_eq!(result.messages.len(), 2);
        assert!(result.messages.iter().all(|m| m.role == MessageRole::User));
    }

    #[test]
    fn test_filter_exclude() {
        let mgr = MessageArrayManager::new(vec![
            msg(MessageRole::System, "sys"),
            msg(MessageRole::User, "q1"),
            msg(MessageRole::Assistant, "a1"),
        ]);
        let result = mgr.execute(MessageOperationConfig::Filter(FilterMessageOperation {
            role: Some(MessageRole::User),
            exclude: Some(true),
            custom_filter: None,
        }));

        assert_eq!(result.messages.len(), 2);
        assert!(!result.messages.iter().any(|m| m.role == MessageRole::User));
    }

    #[test]
    fn test_rollback() {
        let mgr = MessageArrayManager::new(vec![msg(MessageRole::System, "sys")]);

        mgr.execute(MessageOperationConfig::Append(AppendMessageOperation {
            messages: vec![msg(MessageRole::User, "q1")],
            batch_index: None,
        }));
        mgr.execute(MessageOperationConfig::Insert(InsertMessageOperation {
            messages: vec![msg(MessageRole::Assistant, "a1")],
            index: 1,
        }));

        assert_eq!(mgr.current_messages().len(), 3);

        let result = mgr.rollback_to(0);
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, MessageRole::System);
        assert_eq!(result.messages[1].role, MessageRole::User);
    }

    #[test]
    fn test_snapshot_after_structural_change() {
        let mgr = MessageArrayManager::new(vec![msg(MessageRole::System, "sys")]);

        mgr.execute(MessageOperationConfig::Insert(InsertMessageOperation {
            messages: vec![msg(MessageRole::User, "q")],
            index: 1,
        }));

        let snap = mgr.get_batch_snapshot(0);
        assert!(snap.is_some());
        assert_eq!(snap.unwrap().messages.len(), 1);
    }
}
