use wf_common::error_chain::ErrorRecord;
use wf_types::{ErrorType, RecoveryAction};

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("environment variable error: {0}")]
    EnvVar(String),

    #[error("index error: {0}")]
    Index(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl ConfigError {
    /// Classify into the shared error taxonomy so config failures can feed
    /// the structured error-record pipeline used by agent/workflow layers.
    pub fn error_type(&self) -> ErrorType {
        match self {
            ConfigError::Parse(_) | ConfigError::Validation(_) => ErrorType::Validation,
            ConfigError::Serialization(_) | ConfigError::EnvVar(_) => ErrorType::Validation,
            ConfigError::NotFound(_) => ErrorType::Validation,
            ConfigError::Io(_) | ConfigError::Index(_) | ConfigError::Internal(_) => {
                ErrorType::Internal
            }
        }
    }

    /// Recommended recovery action. Config failures are fail-fast: most
    /// require fixing the configuration and re-running; IO errors may be
    /// transient and worth retrying.
    pub fn recovery_action(&self) -> RecoveryAction {
        match self {
            ConfigError::Io(_) => RecoveryAction::Retry,
            _ => RecoveryAction::Abort,
        }
    }

    /// Convert into a standalone structured error record. Config errors do
    /// not participate in cross-execution chains, so the record carries no
    /// parent linkage.
    pub fn to_error_record(&self, execution_id: String, node_id: Option<String>) -> ErrorRecord {
        let id = wf_common::generate_id();
        ErrorRecord {
            id: id.clone(),
            execution_id,
            error: self.to_string(),
            error_type: Some(self.error_type()),
            timestamp: wf_common::now(),
            node_id,
            parent_error_id: None,
            error_chain: vec![id.clone()],
            root_cause_id: id,
            caused_by: None,
            is_recoverable: matches!(
                self.recovery_action(),
                RecoveryAction::Retry | RecoveryAction::Fallback
            ),
            recovery_action: Some(self.recovery_action()),
        }
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> Self {
        ConfigError::Serialization(e.to_string())
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Parse(e.to_string())
    }
}

impl From<regex::Error> for ConfigError {
    fn from(e: regex::Error) -> Self {
        ConfigError::Internal(format!("regex error: {e}"))
    }
}

pub type ConfigResult<T> = Result<T, ConfigError>;
