use std::collections::HashMap;
use std::sync::Mutex;

use wf_types::{ErrorCause, ErrorSeverity, ErrorType, RecoveryAction};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErrorPattern {
    pub total_errors: usize,
    pub type_distribution: HashMap<String, usize>,
    pub severity_distribution: HashMap<String, usize>,
    pub affected_nodes: Vec<String>,
    pub most_common_type: Option<ErrorType>,
    pub most_common_severity: Option<ErrorSeverity>,
    pub has_recoverable: bool,
    pub recovery_action_count: HashMap<String, usize>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ErrorMetadata {
    pub error_type: Option<ErrorType>,
    pub severity: Option<ErrorSeverity>,
    pub caused_by: Option<ErrorCause>,
    pub is_recoverable: bool,
    pub recovery_action: Option<RecoveryAction>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorRecord {
    pub id: String,
    pub execution_id: String,
    pub error: String,
    pub error_type: Option<ErrorType>,
    pub severity: Option<ErrorSeverity>,
    pub timestamp: i64,
    pub node_id: Option<String>,
    pub parent_error_id: Option<String>,
    pub error_chain: Vec<String>,
    pub root_cause_id: String,
    pub caused_by: Option<ErrorCause>,
    pub is_recoverable: bool,
    pub recovery_action: Option<RecoveryAction>,
}

pub struct ErrorChainManager {
    records: Mutex<HashMap<String, Vec<ErrorRecord>>>,
}

impl ErrorChainManager {
    pub fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
        }
    }

    pub fn record(&self, execution_id: String, error: String, node_id: Option<String>) -> String {
        self.record_with_metadata(execution_id, error, node_id, ErrorMetadata::default())
    }

    pub fn record_with_metadata(
        &self,
        execution_id: String,
        error: String,
        node_id: Option<String>,
        metadata: ErrorMetadata,
    ) -> String {
        let id = crate::generate_id();
        let mut records = self.records.lock().unwrap();
        let entry = records.entry(execution_id.clone()).or_default();

        let (error_chain, root_cause_id, parent_error_id) = match entry.last() {
            Some(last) => {
                let mut chain = last.error_chain.clone();
                chain.push(id.clone());
                (chain, last.root_cause_id.clone(), Some(last.id.clone()))
            }
            None => (vec![id.clone()], id.clone(), None),
        };

        entry.push(ErrorRecord {
            id: id.clone(),
            execution_id,
            error,
            error_type: metadata.error_type,
            severity: metadata.severity,
            timestamp: crate::time::now(),
            node_id,
            parent_error_id,
            error_chain,
            root_cause_id,
            caused_by: metadata.caused_by,
            is_recoverable: metadata.is_recoverable,
            recovery_action: metadata.recovery_action,
        });

        id
    }

    pub fn get_records(&self, execution_id: &str) -> Vec<ErrorRecord> {
        self.records
            .lock()
            .unwrap()
            .get(execution_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_chain(&self, execution_id: &str) -> Vec<ErrorRecord> {
        let all = self.get_records(execution_id);
        if all.is_empty() {
            return Vec::new();
        }

        let last = all.last().unwrap();
        let mut chain = Vec::new();
        let mut current_id: Option<&String> = Some(&last.id);

        while let Some(id) = current_id {
            if let Some(record) = all.iter().find(|r| &r.id == id) {
                chain.push(record.clone());
                current_id = record.parent_error_id.as_ref();
            } else {
                break;
            }
        }

        chain.reverse();
        chain
    }

    pub fn get_error_chain_ids(&self, execution_id: &str) -> Vec<String> {
        let all = self.get_records(execution_id);
        match all.last() {
            Some(last) => last.error_chain.clone(),
            None => Vec::new(),
        }
    }

    pub fn count(&self, execution_id: &str) -> usize {
        self.records
            .lock()
            .unwrap()
            .get(execution_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    pub fn clear(&self, execution_id: &str) {
        self.records.lock().unwrap().remove(execution_id);
    }

    pub fn clear_all(&self) {
        self.records.lock().unwrap().clear();
    }

    pub fn analyze_error_pattern(&self, execution_id: &str) -> ErrorPattern {
        let records = self.get_records(execution_id);
        let mut type_dist: HashMap<String, usize> = HashMap::new();
        let mut severity_dist: HashMap<String, usize> = HashMap::new();
        let mut affected_nodes: Vec<String> = Vec::new();
        let mut recovery_count: HashMap<String, usize> = HashMap::new();
        let mut has_recoverable = false;

        for record in &records {
            if let Some(ref error_type) = record.error_type {
                *type_dist.entry(format!("{:?}", error_type)).or_insert(0) += 1;
            }
            if let Some(ref severity) = record.severity {
                *severity_dist.entry(format!("{:?}", severity)).or_insert(0) += 1;
            }
            if let Some(ref node_id) = record.node_id {
                if !affected_nodes.contains(node_id) {
                    affected_nodes.push(node_id.clone());
                }
            }
            if record.is_recoverable {
                has_recoverable = true;
            }
            if let Some(ref action) = record.recovery_action {
                *recovery_count.entry(format!("{:?}", action)).or_insert(0) += 1;
            }
        }

        let most_common_type = type_dist
            .iter()
            .max_by_key(|(_, count)| *count)
            .and_then(|(name, _)| match name.as_str() {
                "ToolError" => Some(ErrorType::ToolError),
                "LlmError" => Some(ErrorType::LlmError),
                "Timeout" => Some(ErrorType::Timeout),
                "Validation" => Some(ErrorType::Validation),
                "Internal" => Some(ErrorType::Internal),
                "Interruption" => Some(ErrorType::Interruption),
                _ => None,
            });

        let most_common_severity = severity_dist
            .iter()
            .max_by_key(|(_, count)| *count)
            .and_then(|(name, _)| match name.as_str() {
                "Info" => Some(ErrorSeverity::Info),
                "Warning" => Some(ErrorSeverity::Warning),
                "Error" => Some(ErrorSeverity::Error),
                "Critical" => Some(ErrorSeverity::Critical),
                _ => None,
            });

        ErrorPattern {
            total_errors: records.len(),
            type_distribution: type_dist,
            severity_distribution: severity_dist,
            affected_nodes,
            most_common_type,
            most_common_severity,
            has_recoverable,
            recovery_action_count: recovery_count,
        }
    }

    pub fn get_recommended_recovery_action(&self, execution_id: &str) -> Option<RecoveryAction> {
        let pattern = self.analyze_error_pattern(execution_id);

        if !pattern.has_recoverable {
            return Some(RecoveryAction::Abort);
        }

        pattern
            .recovery_action_count
            .iter()
            .max_by_key(|(_, count)| *count)
            .and_then(|(name, _)| match name.as_str() {
                "Retry" => Some(RecoveryAction::Retry),
                "Fallback" => Some(RecoveryAction::Fallback),
                "ManualIntervention" => Some(RecoveryAction::ManualIntervention),
                "Abort" => Some(RecoveryAction::Abort),
                _ => None,
            })
    }
}

impl Default for ErrorChainManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_get() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        let id1 = manager.record(exec_id.clone(), "error 1".to_string(), None);
        let id2 = manager.record(exec_id.clone(), "error 2".to_string(), Some("node-a".to_string()));

        let records = manager.get_records(&exec_id);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].error, "error 1");
        assert_eq!(records[1].node_id.as_deref(), Some("node-a"));

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_auto_chain_building() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record(exec_id.clone(), "root cause".to_string(), Some("node-1".to_string()));
        manager.record(exec_id.clone(), "intermediate".to_string(), Some("node-2".to_string()));
        manager.record(exec_id.clone(), "top error".to_string(), Some("node-3".to_string()));

        let chain = manager.get_chain(&exec_id);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].error, "root cause");
        assert_eq!(chain[1].error, "intermediate");
        assert_eq!(chain[2].error, "top error");
    }

    #[test]
    fn test_error_chain_ids() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        let id1 = manager.record(exec_id.clone(), "error 1".to_string(), None);
        let id2 = manager.record(exec_id.clone(), "error 2".to_string(), None);
        let id3 = manager.record(exec_id.clone(), "error 3".to_string(), None);

        let chain_ids = manager.get_error_chain_ids(&exec_id);
        assert_eq!(chain_ids, vec![id1, id2, id3]);
    }

    #[test]
    fn test_root_cause_id() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        let root_id = manager.record(exec_id.clone(), "root".to_string(), None);
        let mid_id = manager.record(exec_id.clone(), "middle".to_string(), None);
        let top_id = manager.record(exec_id.clone(), "top".to_string(), None);

        let records = manager.get_records(&exec_id);
        assert_eq!(records[0].root_cause_id, root_id);
        assert_eq!(records[1].root_cause_id, root_id);
        assert_eq!(records[2].root_cause_id, root_id);

        assert!(records[0].parent_error_id.is_none());
        assert_eq!(records[1].parent_error_id, Some(root_id.clone()));
        assert_eq!(records[2].parent_error_id, Some(mid_id.clone()));

        assert_eq!(records[0].error_chain, vec![root_id.clone()]);
        assert_eq!(records[1].error_chain, vec![root_id.clone(), mid_id.clone()]);
        assert_eq!(records[2].error_chain, vec![root_id.clone(), mid_id.clone(), top_id]);
    }

    #[test]
    fn test_get_chain_empty() {
        let manager = ErrorChainManager::new();
        let chain = manager.get_chain("nonexistent");
        assert!(chain.is_empty());
    }

    #[test]
    fn test_count() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        assert_eq!(manager.count(&exec_id), 0);
        manager.record(exec_id.clone(), "error".to_string(), None);
        assert_eq!(manager.count(&exec_id), 1);
    }

    #[test]
    fn test_clear() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record(exec_id.clone(), "error".to_string(), None);
        manager.clear(&exec_id);
        assert!(manager.get_records(&exec_id).is_empty());
    }

    #[test]
    fn test_clear_all() {
        let manager = ErrorChainManager::new();
        manager.record("exec-1".to_string(), "error".to_string(), None);
        manager.record("exec-2".to_string(), "error".to_string(), None);
        manager.clear_all();
        assert!(manager.get_records("exec-1").is_empty());
    }

    #[test]
    fn test_record_with_metadata() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        let id = manager.record_with_metadata(
            exec_id.clone(),
            "timeout error".to_string(),
            Some("node-1".to_string()),
            ErrorMetadata {
                error_type: Some(ErrorType::Timeout),
                severity: Some(ErrorSeverity::Error),
                caused_by: Some(ErrorCause {
                    reason: "LLM call exceeded 30s".to_string(),
                    handling_attempt: Some("retry_1".to_string()),
                }),
                is_recoverable: true,
                recovery_action: Some(RecoveryAction::Retry),
            },
        );

        let records = manager.get_records(&exec_id);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id);
        assert!(matches!(records[0].error_type, Some(ErrorType::Timeout)));
        assert!(matches!(records[0].severity, Some(ErrorSeverity::Error)));
        assert!(records[0].is_recoverable);
        assert!(matches!(records[0].recovery_action, Some(RecoveryAction::Retry)));
    }

    #[test]
    fn test_analyze_error_pattern() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record_with_metadata(
            exec_id.clone(),
            "timeout 1".to_string(),
            Some("node-a".to_string()),
            ErrorMetadata {
                error_type: Some(ErrorType::Timeout),
                severity: Some(ErrorSeverity::Error),
                caused_by: None,
                is_recoverable: true,
                recovery_action: Some(RecoveryAction::Retry),
            },
        );

        manager.record_with_metadata(
            exec_id.clone(),
            "timeout 2".to_string(),
            Some("node-b".to_string()),
            ErrorMetadata {
                error_type: Some(ErrorType::Timeout),
                severity: Some(ErrorSeverity::Critical),
                caused_by: None,
                is_recoverable: true,
                recovery_action: Some(RecoveryAction::Retry),
            },
        );

        manager.record_with_metadata(
            exec_id.clone(),
            "validation error".to_string(),
            Some("node-a".to_string()),
            ErrorMetadata {
                error_type: Some(ErrorType::Validation),
                severity: Some(ErrorSeverity::Warning),
                caused_by: None,
                is_recoverable: false,
                recovery_action: Some(RecoveryAction::ManualIntervention),
            },
        );

        let pattern = manager.analyze_error_pattern(&exec_id);
        assert_eq!(pattern.total_errors, 3);
        assert_eq!(pattern.type_distribution.get("Timeout").copied().unwrap_or(0), 2);
        assert_eq!(pattern.type_distribution.get("Validation").copied().unwrap_or(0), 1);
        assert_eq!(pattern.most_common_type, Some(ErrorType::Timeout));
        assert!(pattern.has_recoverable);
        assert!(pattern.affected_nodes.contains(&"node-a".to_string()));
        assert!(pattern.affected_nodes.contains(&"node-b".to_string()));
        assert_eq!(pattern.recovery_action_count.get("Retry").copied().unwrap_or(0), 2);
    }

    #[test]
    fn test_analyze_empty_pattern() {
        let manager = ErrorChainManager::new();
        let pattern = manager.analyze_error_pattern("nonexistent");
        assert_eq!(pattern.total_errors, 0);
        assert!(pattern.type_distribution.is_empty());
        assert!(pattern.affected_nodes.is_empty());
        assert!(!pattern.has_recoverable);
    }

    #[test]
    fn test_get_recommended_recovery_action() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record_with_metadata(
            exec_id.clone(),
            "error 1".to_string(),
            None,
            ErrorMetadata {
                error_type: Some(ErrorType::Timeout),
                severity: Some(ErrorSeverity::Error),
                caused_by: None,
                is_recoverable: true,
                recovery_action: Some(RecoveryAction::Retry),
            },
        );

        manager.record_with_metadata(
            exec_id.clone(),
            "error 2".to_string(),
            None,
            ErrorMetadata {
                error_type: Some(ErrorType::Timeout),
                severity: Some(ErrorSeverity::Error),
                caused_by: None,
                is_recoverable: true,
                recovery_action: Some(RecoveryAction::Retry),
            },
        );

        manager.record_with_metadata(
            exec_id.clone(),
            "error 3".to_string(),
            None,
            ErrorMetadata {
                error_type: Some(ErrorType::LlmError),
                severity: Some(ErrorSeverity::Critical),
                caused_by: None,
                is_recoverable: false,
                recovery_action: Some(RecoveryAction::Abort),
            },
        );

        let action = manager.get_recommended_recovery_action(&exec_id);
        assert!(matches!(action, Some(RecoveryAction::Retry)));
    }

    #[test]
    fn test_get_recommended_recovery_action_no_recoverable() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record_with_metadata(
            exec_id.clone(),
            "fatal".to_string(),
            None,
            ErrorMetadata {
                error_type: Some(ErrorType::Internal),
                severity: Some(ErrorSeverity::Critical),
                caused_by: None,
                is_recoverable: false,
                recovery_action: None,
            },
        );

        let action = manager.get_recommended_recovery_action(&exec_id);
        assert!(matches!(action, Some(RecoveryAction::Abort)));
    }
}
