use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserGitConfig {
    #[serde(default)]
    pub enabled: bool,
    pub repo_root: Option<String>,
    #[serde(default = "default_command_timeout_ms")]
    pub command_timeout_ms: u64,
    #[serde(default = "default_remote_timeout_ms")]
    pub remote_timeout_ms: u64,
}

fn default_command_timeout_ms() -> u64 {
    30_000
}

fn default_remote_timeout_ms() -> u64 {
    120_000
}

impl Default for UserGitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            repo_root: None,
            command_timeout_ms: default_command_timeout_ms(),
            remote_timeout_ms: default_remote_timeout_ms(),
        }
    }
}
