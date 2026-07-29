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
    #[error("plugin '{plugin_id}' in state '{state}'")]
    InvalidState { plugin_id: String, state: String },
    #[error("dependency not satisfied: {0}")]
    DependencyNotSatisfied(String),
    #[error("circular dependency detected")]
    CircularDependency,
    #[error("contribution conflict: {0}")]
    ContributionConflict(String),
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
