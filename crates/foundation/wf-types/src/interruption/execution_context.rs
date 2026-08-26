use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionDomainContext {
    pub domain_type: String,
    pub domain_id: String,
    pub state: serde_json::Value,
}
