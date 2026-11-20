use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IndexType {
    LlmProfiles,
    LlmProviders,
    LlmAuth,
}

/// Index file schema matching the `index.json` format: `version` +
/// `type` + `paths`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigIndexFile {
    pub version: String,
    #[serde(rename = "type")]
    pub index_type: IndexType,
    pub paths: Vec<String>,
}
