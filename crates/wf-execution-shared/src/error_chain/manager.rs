use std::collections::HashMap;
use std::sync::Mutex;

use wf_types::Id;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorRecord {
    pub execution_id: Id,
    pub error: String,
    pub timestamp: i64,
    pub node_id: Option<String>,
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
        };
        self.records.lock().unwrap()
            .entry(execution_id)
            .or_default()
            .push(record);
    }

    pub fn get_records(&self, execution_id: &Id) -> Vec<ErrorRecord> {
        self.records.lock().unwrap()
            .get(execution_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn clear(&self, execution_id: &Id) {
        self.records.lock().unwrap().remove(execution_id);
    }
}
