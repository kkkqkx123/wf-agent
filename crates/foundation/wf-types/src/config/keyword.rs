use serde::{Deserialize, Serialize};

/// Keyword index configuration. The index is enabled by default and
/// resolves its database path from the storage configuration; an explicit
/// path wins, and disabling keeps hybrid retrieval off while dense
/// retrieval keeps working.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeywordIndexConfig {
    /// Master switch. `false` leaves the keyword slot empty.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Explicit database path. `None` derives a sibling of the storage
    /// database from the storage configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl Default for KeywordIndexConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            db_path: None,
        }
    }
}
