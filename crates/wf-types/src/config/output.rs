use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputConfig {
    pub format: Option<String>,
    pub include_metadata: Option<bool>,
    pub include_errors: Option<bool>,
    pub max_output_size: Option<u64>,
}
