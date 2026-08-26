use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SdkLogLevel {
    Silent,
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Json,
    Table,
    Plain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputConfig {
    pub dir: String,
    pub log_file_pattern: String,
    pub enable_log_terminal: bool,
    pub enable_sdk_logs: bool,
    pub sdk_log_level: SdkLogLevel,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            dir: "./outputs".to_string(),
            log_file_pattern: "app-{date}.log".to_string(),
            enable_log_terminal: true,
            enable_sdk_logs: true,
            sdk_log_level: SdkLogLevel::Warn,
        }
    }
}
