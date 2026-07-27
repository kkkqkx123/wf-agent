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
