use std::collections::HashMap;
use std::sync::Mutex;

use wf_types::Id;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorRecord {
    pub execution_id: Id,
    pub error: String,
    pub timestamp: i64,
    pub node_id: Option<String>,
    pub parent_error_index: Option<usize>,
}

pub struct ErrorChainManager {
    records: Mutex<HashMap<Id, Vec<ErrorRecord>>>,
}

impl ErrorChainManager {
    pub fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
        }
    }

    pub fn record(&self, execution_id: Id, error: String, node_id: Option<String>) {
        let record = ErrorRecord {
            execution_id: execution_id.clone(),
            error,
            timestamp: wf_common::now(),
            node_id,
            parent_error_index: None,
        };
        self.records
            .lock()
            .unwrap()
            .entry(execution_id)
            .or_default()
            .push(record);
    }

    pub fn record_chained(
        &self,
        execution_id: Id,
        error: String,
        node_id: Option<String>,
        parent_error_index: usize,
    ) {
        let record = ErrorRecord {
            execution_id: execution_id.clone(),
            error,
            timestamp: wf_common::now(),
            node_id,
            parent_error_index: Some(parent_error_index),
        };
        self.records
            .lock()
            .unwrap()
            .entry(execution_id)
            .or_default()
            .push(record);
    }

    pub fn get_records(&self, execution_id: &Id) -> Vec<ErrorRecord> {
        self.records
            .lock()
            .unwrap()
            .get(execution_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_chain(&self, execution_id: &Id) -> Vec<ErrorRecord> {
        let all = self.get_records(execution_id);
        if all.is_empty() {
            return Vec::new();
        }

        let last_index = all.len() - 1;
        let mut chain = Vec::new();
        let mut current = Some(last_index);

        while let Some(idx) = current {
            if idx < all.len() {
                chain.push(all[idx].clone());
                current = all[idx].parent_error_index;
            } else {
                break;
            }
        }

        chain.reverse();
        chain
    }

    pub fn count(&self, execution_id: &Id) -> usize {
        self.records
            .lock()
            .unwrap()
            .get(execution_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    pub fn clear(&self, execution_id: &Id) {
        self.records.lock().unwrap().remove(execution_id);
    }

    pub fn clear_all(&self) {
        self.records.lock().unwrap().clear();
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

        manager.record(exec_id.clone(), "error 1".to_string(), None);
        manager.record(exec_id.clone(), "error 2".to_string(), Some("node-a".to_string()));

        let records = manager.get_records(&exec_id);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].error, "error 1");
        assert_eq!(records[1].node_id.as_deref(), Some("node-a"));
    }

    #[test]
    fn test_get_chain() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record(exec_id.clone(), "root cause".to_string(), Some("node-1".to_string()));
        manager.record_chained(exec_id.clone(), "intermediate".to_string(), Some("node-2".to_string()), 0);
        manager.record_chained(exec_id.clone(), "top error".to_string(), Some("node-3".to_string()), 1);

        let chain = manager.get_chain(&exec_id);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].error, "root cause");
        assert_eq!(chain[1].error, "intermediate");
        assert_eq!(chain[2].error, "top error");
    }

    #[test]
    fn test_get_chain_empty() {
        let manager = ErrorChainManager::new();
        let chain = manager.get_chain(&"nonexistent".to_string());
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
        assert!(manager.get_records(&"exec-1".to_string()).is_empty());
    }

    #[test]
    fn test_chained_without_parent() {
        let manager = ErrorChainManager::new();
        let exec_id = "exec-1".to_string();

        manager.record(exec_id.clone(), "error 1".to_string(), None);
        manager.record(exec_id.clone(), "error 2".to_string(), None);

        let chain = manager.get_chain(&exec_id);
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].error, "error 2");
    }
}
