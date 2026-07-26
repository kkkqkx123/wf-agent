use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTemplateStorageConfig {
    pub storage_type: String,
}
