use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("load failed: {0}")]
    LoadFailed(String),
    #[error("activation failed: {0}")]
    ActivationFailed(String),
    #[error("deactivation failed: {0}")]
    DeactivationFailed(String),
    #[error("timeout on plugin '{plugin_id}'")]
    Timeout { plugin_id: String },
    #[error("plugin '{plugin_id}' panicked during execution")]
    PluginPanic { plugin_id: String },
    #[error("invalid contribution from plugin '{plugin_id}': {message}")]
    InvalidContribution { plugin_id: String, message: String },
    #[error("plugin '{plugin_id}' in state '{state}'")]
    InvalidState { plugin_id: String, state: String },
    #[error("dependency not satisfied: {0}")]
    DependencyNotSatisfied(String),
    #[error("circular dependency detected")]
    CircularDependency,
    #[error("contribution conflict: {0}")]
    ContributionConflict(String),
    #[error("config change failed for plugin '{plugin_id}': {message}")]
    ConfigChangeFailed { plugin_id: String, message: String },
    #[error("lua error: {0}")]
    LuaError(String),
    #[error("native error: {0}")]
    NativeError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("internal: {0}")]
    Internal(String),
}

pub type PluginResult<T> = Result<T, PluginError>;
