use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointVariableState {
    pub variables: HashMap<String, serde_json::Value>,
}
