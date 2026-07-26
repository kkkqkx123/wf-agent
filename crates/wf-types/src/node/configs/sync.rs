use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncNodeConfig {
    pub sync_point: String,
    pub timeout: Option<u64>,
}
