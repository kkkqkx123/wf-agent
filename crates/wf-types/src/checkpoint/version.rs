use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointFormatVersion {
    pub version: String,
    pub min_compatible_version: String,
}
