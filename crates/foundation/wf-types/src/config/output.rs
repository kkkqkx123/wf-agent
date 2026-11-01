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
    #[serde(default = "default_output_dir")]
    pub dir: String,
    #[serde(default = "default_log_file_pattern")]
    pub log_file_pattern: String,
    #[serde(default = "default_true")]
    pub enable_log_terminal: bool,
    #[serde(default = "default_true")]
    pub enable_sdk_logs: bool,
    #[serde(default = "default_sdk_log_level")]
    pub sdk_log_level: SdkLogLevel,
}

fn default_output_dir() -> String {
    "./outputs".to_string()
}

fn default_log_file_pattern() -> String {
    "app-{date}.log".to_string()
}

fn default_true() -> bool {
    true
}

fn default_sdk_log_level() -> SdkLogLevel {
    SdkLogLevel::Warn
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
