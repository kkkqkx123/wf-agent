//! CLI error type and process exit-code mapping.
//!
//! Exit codes follow `sysexits.h`, centralized in `wf_common::exit_code`
//! and shared with every CLI frontend (including the offline debugger).
//! See that module for the authoritative table; do not hardcode numbers.

use std::fmt;

use wf_common::exit_code as exit;
use wf_common::CommonError;

/// Errors surfaced by the CLI layer.
#[derive(Debug)]
pub enum CliError {
    /// Invalid command line arguments or incompatible option combination.
    /// Maps to `EX_USAGE`.
    Arguments(String),
    /// Bad flags, incompatible versions, affinity mismatch, missing
    /// credentials or not-configured state. Maps to `EX_CONFIG`.
    Configuration(String),
    /// Terminal backend failure (init, draw, poll, event read).
    /// Maps to `EX_SOFTWARE`.
    Terminal(String),
    /// Service unreachable: local relay or remote service alike
    /// (connection refused, timeout, health probe failure). Maps to
    /// `EX_UNAVAILABLE`.
    Unavailable(String),
    /// Internal fault: serialization or other operational errors that are
    /// not the user's fault and not a service outage. Maps to `EX_SOFTWARE`.
    Internal(String),
    /// Business-level failure of a session / command. Maps to `EX_DATAERR`.
    Business(String),
    /// Remote business failure whose exit code is derived from the stable
    /// remote business code via [`exit::remote_exit_code`]. The carried
    /// `code` is the sysexits value; `remote` is the stable server code kept
    /// for diagnostics.
    RemoteBusiness {
        code: String,
        message: String,
        exit_code: u8,
    },
    /// Execution was interrupted (SIGINT / user cancel).
    /// Maps to POSIX `128 + SIGINT`.
    Interrupted(String),
    /// Underlying IO error while writing output. Maps to `EX_IOERR`.
    Io(std::io::Error),
    /// Underlying common error forwarded unchanged. Maps to `EX_SOFTWARE`.
    Common(CommonError),
}

impl CliError {
    /// Process exit code for this error, from the shared sysexits contract.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Arguments(_) => exit::USAGE,
            Self::Configuration(_) => exit::CONFIG,
            Self::Terminal(_) => exit::SOFTWARE,
            Self::Unavailable(_) => exit::UNAVAILABLE,
            Self::Internal(_) => exit::SOFTWARE,
            Self::Business(_) => exit::DATAERR,
            Self::RemoteBusiness { exit_code, .. } => *exit_code,
            Self::Interrupted(_) => exit::INTERRUPTED,
            Self::Io(_) => exit::IOERR,
            Self::Common(_) => exit::SOFTWARE,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arguments(msg) => write!(f, "invalid arguments: {msg}"),
            Self::Configuration(msg) => write!(f, "configuration error: {msg}"),
            Self::Terminal(msg) => write!(f, "terminal error: {msg}"),
            Self::Unavailable(msg) => write!(f, "service unavailable: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
            Self::Business(msg) => write!(f, "command failed: {msg}"),
            Self::RemoteBusiness { code, message, .. } => {
                write!(f, "command failed: remote {code}: {message}")
            }
            Self::Interrupted(msg) => write!(f, "interrupted: {msg}"),
            Self::Io(err) => write!(f, "io error: {err}"),
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

impl From<CommonError> for CliError {
    fn from(err: CommonError) -> Self {
        Self::Common(err)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(err: serde_json::Error) -> Self {
        // Serialization is an internal fault, not a user-facing business
        // outcome. File-parse call sites map to `Arguments` explicitly.
        Self::Internal(format!("serialization error: {err}"))
    }
}

impl From<wf_client::ClientError> for CliError {
    fn from(err: wf_client::ClientError) -> Self {
        use wf_client::ClientError as C;
        match err {
            C::Remote { code, message } => {
                let hint = match code.as_str() {
                    "SESSION_BUSY" => "; session is busy, retry after it settles",
                    "SESSION_NOT_FOUND" => "; check the session id",
                    "LOCATION_NOT_FOUND" => "; check --directory / WF_DIRECTORY",
                    "FORBIDDEN" => {
                        "; directory is not served, check server allowlist and `wf directories`"
                    }
                    "TICKET_INVALID" => "; ticket expired, retry the handshake",
                    "SERVICE_UNAVAILABLE" => "; server not ready or operation unsupported",
                    _ => "",
                };
                // A local relay and a remote service are equivalent: the
                // business code alone decides the exit code from the shared
                // table, never where the server happens to be running.
                let exit_code = exit::remote_exit_code(&code);
                Self::RemoteBusiness {
                    code,
                    message: format!("{message}{hint}"),
                    exit_code,
                }
            }
            // Transport failures are service outages, not misconfiguration.
            C::Http(msg) => Self::Unavailable(format!("remote http: {msg}")),
            C::Stream(msg) => Self::Unavailable(format!("stream error: {msg}")),
            C::Invalid(msg) => Self::Configuration(format!("remote invalid: {msg}")),
            C::NotFound(what) => Self::Business(format!("not found: {what}")),
            C::Unauthorized => Self::Configuration(
                "server rejected the credentials; set --api-key / WF_API_KEY".to_string(),
            ),
            C::NotConfigured(msg) => Self::Configuration(msg),
        }
    }
}

/// Result alias for CLI operations.
pub type CliResult<T> = Result<T, CliError>;

/// Print a CLI failure to stderr and terminate with its mapped sysexits
/// code. Shared by every frontend so the stderr/exit behavior cannot drift.
///
/// The `label` is the invoked binary name (e.g. `wf`, `wf-headless`).
pub fn exit_with_error(label: &str, err: &CliError) -> ! {
    eprintln!("{label}: {err}");
    std::process::exit(i32::from(err.exit_code()));
}

/// Parse command-line arguments for a frontend, converting clap's own
/// parse error into [`CliError::Arguments`] so usage errors use the shared
/// `EX_USAGE` code instead of clap's built-in `2`.
///
/// `command` is the fully configured clap [`clap::Command`] and `label` the
/// invoked binary name. `--help` / `--version` still exit successfully via
/// clap's `Error::exit()`; only usage errors are converted.
pub fn parse_cli_or_exit<T, F>(command: clap::Command, label: &str, parse: F) -> T
where
    F: FnOnce(&clap::ArgMatches) -> Result<T, clap::Error>,
{
    use clap::error::ErrorKind;
    let matches = match command.try_get_matches() {
        Ok(matches) => matches,
        Err(err) => match err.kind() {
            // Help and version are successful outcomes, not usage errors.
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => err.exit(),
            _ => exit_with_error(label, &CliError::Arguments(err.to_string())),
        },
    };
    match parse(&matches) {
        Ok(value) => value,
        Err(err) => {
            let kind = err.kind();
            match kind {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => err.exit(),
                _ => exit_with_error(label, &CliError::Arguments(err.to_string())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_client::ClientError as C;

    #[test]
    fn exit_codes_match_documented_mapping() {
        assert_eq!(CliError::Arguments("x".into()).exit_code(), exit::USAGE);
        assert_eq!(
            CliError::Configuration("x".into()).exit_code(),
            exit::CONFIG
        );
        assert_eq!(CliError::Terminal("x".into()).exit_code(), exit::SOFTWARE);
        assert_eq!(
            CliError::Unavailable("x".into()).exit_code(),
            exit::UNAVAILABLE
        );
        assert_eq!(CliError::Internal("x".into()).exit_code(), exit::SOFTWARE);
        assert_eq!(CliError::Business("x".into()).exit_code(), exit::DATAERR);
        assert_eq!(
            CliError::Interrupted("x".into()).exit_code(),
            exit::INTERRUPTED
        );
        assert_eq!(
            CliError::Io(std::io::Error::other("x")).exit_code(),
            exit::IOERR
        );
        assert_eq!(
            CliError::Common(CommonError::Internal("x".into())).exit_code(),
            exit::SOFTWARE
        );

        let json_err = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        assert_eq!(
            CliError::from(json_err).exit_code(),
            exit::SOFTWARE,
            "serialization faults are internal errors"
        );
    }

    #[test]
    fn transport_failures_map_to_unavailable() {
        assert_eq!(
            CliError::from(C::Http("refused".into())).exit_code(),
            exit::UNAVAILABLE
        );
        assert_eq!(
            CliError::from(C::Stream("reset".into())).exit_code(),
            exit::UNAVAILABLE
        );
    }

    #[test]
    fn remote_business_codes_map_to_distinct_exit_codes() {
        let remote = |code: &str| {
            CliError::from(C::Remote {
                code: code.to_string(),
                message: "m".to_string(),
            })
        };
        assert_eq!(remote("SESSION_BUSY").exit_code(), exit::TEMPFAIL);
        assert_eq!(remote("SESSION_NOT_FOUND").exit_code(), exit::NOUSER);
        assert_eq!(remote("LOCATION_NOT_FOUND").exit_code(), exit::NOINPUT);
        assert_eq!(remote("FORBIDDEN").exit_code(), exit::NOPERM);
        assert_eq!(remote("TICKET_INVALID").exit_code(), exit::NOPERM);
        assert_eq!(remote("SERVICE_UNAVAILABLE").exit_code(), exit::UNAVAILABLE);
        assert_eq!(remote("BRAND_NEW_CODE").exit_code(), exit::PROTOCOL);
    }
}
