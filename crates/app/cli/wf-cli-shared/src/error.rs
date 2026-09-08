//! CLI error type and process exit-code mapping.
//!
//! Exit codes:
//!   0 - success
//!   1 - business failure (agent session failed)
//!   2 - invalid arguments (also used by clap for parse errors)
//!   3 - configuration error (bootstrap / config load failure)
//!   4 - interrupted (SIGINT during a session)

use std::fmt;

use wf_api::ApiError;
use wf_common::CommonError;
use wf_runtime::error::RuntimeError;

/// Errors surfaced by the CLI layer.
#[derive(Debug)]
pub enum CliError {
    /// Invalid command line arguments or incompatible option combination.
    Arguments(String),
    /// Configuration or runtime bootstrap failure.
    Configuration(String),
    /// Business-level failure of a session / command.
    Business(String),
    /// Execution was interrupted (SIGINT / user cancel).
    Interrupted(String),
    /// Underlying IO error while writing output.
    Io(std::io::Error),
    /// Underlying infrastructure error forwarded unchanged.
    Runtime(RuntimeError),
    /// Underlying API error forwarded unchanged.
    Api(ApiError),
    /// Underlying common error forwarded unchanged.
    Common(CommonError),
}

impl CliError {
    /// Process exit code for this error.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Arguments(_) => 2,
            Self::Configuration(_) => 3,
            Self::Business(_) => 1,
            Self::Interrupted(_) => 4,
            Self::Io(_) => 1,
            Self::Runtime(_) => 1,
            Self::Api(_) => 1,
            Self::Common(_) => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arguments(msg) => write!(f, "invalid arguments: {msg}"),
            Self::Configuration(msg) => write!(f, "configuration error: {msg}"),
            Self::Business(msg) => write!(f, "command failed: {msg}"),
            Self::Interrupted(msg) => write!(f, "interrupted: {msg}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Runtime(err) => write!(f, "runtime error: {err}"),
            Self::Api(err) => write!(f, "api error: {err}"),
            Self::Common(err) => write!(f, "error: {err}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<RuntimeError> for CliError {
    fn from(err: RuntimeError) -> Self {
        Self::Runtime(err)
    }
}

impl From<ApiError> for CliError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl From<CommonError> for CliError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(err: serde_json::Error) -> Self {
        Self::Configuration(format!("serialization error: {err}"))
    }
}

/// Result alias for CLI operations.
pub type CliResult<T> = Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_documented_mapping() {
        assert_eq!(CliError::Arguments("x".into()).exit_code(), 2);
        assert_eq!(CliError::Configuration("x".into()).exit_code(), 3);
        assert_eq!(CliError::Business("x".into()).exit_code(), 1);
        assert_eq!(CliError::Interrupted("x".into()).exit_code(), 4);
        assert_eq!(CliError::Io(std::io::Error::other("x")).exit_code(), 1);
    }
}
