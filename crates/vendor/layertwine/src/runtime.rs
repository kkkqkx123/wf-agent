//! Runtime mode abstraction for Layertwine
//!
//! Provides unified entry point for different runtime modes:
//! - CLI mode: Command-line interface
//! - HTTP mode: REST/JSON API server
//! - gRPC mode: gRPC server

use std::env;
use std::net::SocketAddr;

use crate::error::LayertwineError;

/// Runtime mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Command-line interface mode
    Cli,
    /// HTTP server mode
    Http,
    /// gRPC server mode
    Grpc,
}

/// Common configuration shared across all modes
#[derive(Debug, Clone)]
pub struct CommonConfig {
    /// Path to the layertwine database file
    pub db_path: String,
    /// Path to the Git repository (optional)
    pub git_repo: Option<String>,
}

/// Parse common configuration from environment variables
fn parse_common_config() -> Result<CommonConfig, LayertwineError> {
    let db_path =
        env::var("LAYERTWINE_DB_PATH").unwrap_or_else(|_| ".layertwine/layertwine.db".to_string());
    let git_repo = env::var("LAYERTWINE_GIT_REPO").ok();

    Ok(CommonConfig { db_path, git_repo })
}

/// Detect run mode from environment variables
fn detect_run_mode(_config: &CommonConfig) -> Result<RunMode, LayertwineError> {
    // Honor LAYERTWINE_MODE whenever the CLI binary also carries a server
    // transport (http or grpc). Previously this branch was gated on
    // `all(feature = "cli", feature = "http")`, which silently ignored
    // LAYERTWINE_MODE=grpc under the `cli-grpc` feature combination and
    // always fell back to CLI — breaking embedded gRPC deployment.
    #[cfg(all(feature = "cli", any(feature = "http", feature = "grpc")))]
    {
        if let Ok(mode) = env::var("LAYERTWINE_MODE") {
            match mode.to_lowercase().as_str() {
                "cli" => return Ok(RunMode::Cli),
                "http" => return Ok(RunMode::Http),
                "grpc" => {
                    #[cfg(feature = "grpc")]
                    return Ok(RunMode::Grpc);
                    #[cfg(not(feature = "grpc"))]
                    {
                        return Err(LayertwineError::General(
                            "gRPC mode requires 'grpc' feature".into(),
                        ));
                    }
                }
                _ => {
                    return Err(LayertwineError::General(format!(
                        "unknown mode '{}': expected 'cli', 'http', or 'grpc'",
                        mode
                    )))
                }
            }
        }
    }

    // Default mode selection based on available features
    #[cfg(feature = "cli")]
    {
        Ok(RunMode::Cli)
    }
    #[cfg(all(not(feature = "cli"), feature = "http"))]
    {
        Ok(RunMode::Http)
    }
    #[cfg(all(not(feature = "cli"), not(feature = "http"), feature = "grpc"))]
    {
        Ok(RunMode::Grpc)
    }
    #[cfg(all(not(feature = "cli"), not(feature = "http"), not(feature = "grpc")))]
    {
        Err(LayertwineError::General(
            "No transport feature enabled. Enable at least one of: cli, http, grpc".into(),
        ))
    }
}

/// Parse HTTP server bind address
#[cfg(feature = "http")]
fn parse_http_addr() -> Result<SocketAddr, LayertwineError> {
    let addr_str =
        env::var("LAYERTWINE_HTTP_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    addr_str.parse().map_err(|e| {
        LayertwineError::General(format!("invalid HTTP bind address '{}': {}", addr_str, e))
    })
}

/// Parse gRPC server bind address
#[cfg(feature = "grpc")]
fn parse_grpc_addr() -> Result<SocketAddr, LayertwineError> {
    let addr_str =
        env::var("LAYERTWINE_GRPC_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".to_string());

    addr_str.parse().map_err(|e| {
        LayertwineError::General(format!("invalid gRPC bind address '{}': {}", addr_str, e))
    })
}

/// Common runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Common configuration (db_path, git_repo, etc.)
    pub common: CommonConfig,
    /// Runtime mode
    pub mode: RunMode,
    /// Server bind address (for HTTP/gRPC modes)
    pub bind_addr: Option<SocketAddr>,
}

impl RuntimeConfig {
    /// Parse configuration from command-line arguments
    pub fn parse() -> Result<Self, LayertwineError> {
        let common = parse_common_config()?;
        let mode = detect_run_mode(&common)?;

        let bind_addr = match mode {
            RunMode::Http => {
                #[cfg(feature = "http")]
                {
                    Some(parse_http_addr()?)
                }
                #[cfg(not(feature = "http"))]
                {
                    return Err(LayertwineError::General(
                        "HTTP mode requires 'http' feature".into(),
                    ));
                }
            }
            RunMode::Grpc => {
                #[cfg(feature = "grpc")]
                {
                    Some(parse_grpc_addr()?)
                }
                #[cfg(not(feature = "grpc"))]
                {
                    return Err(LayertwineError::General(
                        "gRPC mode requires 'grpc' feature".into(),
                    ));
                }
            }
            RunMode::Cli => None,
        };

        Ok(RuntimeConfig {
            common,
            mode,
            bind_addr,
        })
    }
}

/// Main entry point for Layertwine (synchronous version for CLI mode)
///
/// Parses configuration, creates the service, and runs in the appropriate mode.
/// This function is used when only CLI mode is available.
pub fn run_sync() -> Result<(), LayertwineError> {
    let config = RuntimeConfig::parse()?;

    match config.mode {
        RunMode::Cli => {
            #[cfg(feature = "cli")]
            {
                run_cli_sync(&config.common)
            }
            #[cfg(not(feature = "cli"))]
            {
                Err(LayertwineError::General(
                    "CLI mode requires 'cli' feature".into(),
                ))
            }
        }
        RunMode::Http | RunMode::Grpc => Err(LayertwineError::General(
            "Async runtime required for HTTP/gRPC mode. Use run_async() instead.".into(),
        )),
    }
}

/// Main entry point for Layertwine (asynchronous version)
///
/// Parses configuration, creates the service, and runs in the appropriate mode.
/// This function is used when HTTP or gRPC mode is available.
pub async fn run_async() -> Result<(), LayertwineError> {
    let config = RuntimeConfig::parse()?;

    match config.mode {
        RunMode::Cli => {
            #[cfg(feature = "cli")]
            {
                run_cli(&config.common).await
            }
            #[cfg(not(feature = "cli"))]
            {
                Err(LayertwineError::General(
                    "CLI mode requires 'cli' feature".into(),
                ))
            }
        }
        RunMode::Http => {
            #[cfg(feature = "http")]
            {
                let addr = config
                    .bind_addr
                    .expect("bind_addr should be set for HTTP mode");
                run_http(&config.common, addr).await
            }
            #[cfg(not(feature = "http"))]
            {
                Err(LayertwineError::General(
                    "HTTP mode requires 'http' feature".into(),
                ))
            }
        }
        RunMode::Grpc => {
            #[cfg(feature = "grpc")]
            {
                let addr = config
                    .bind_addr
                    .expect("bind_addr should be set for gRPC mode");
                run_grpc(&config.common, addr).await
            }
            #[cfg(not(feature = "grpc"))]
            {
                Err(LayertwineError::General(
                    "gRPC mode requires 'grpc' feature".into(),
                ))
            }
        }
    }
}

/// Run in CLI mode (synchronous)
#[cfg(feature = "cli")]
fn run_cli_sync(_config: &CommonConfig) -> Result<(), LayertwineError> {
    let exit_code = crate::cli::run();
    if exit_code == 0 {
        Ok(())
    } else {
        Err(LayertwineError::General(format!(
            "CLI exited with code {}",
            exit_code
        )))
    }
}

/// Run in CLI mode (async wrapper)
#[cfg(feature = "cli")]
async fn run_cli(_config: &CommonConfig) -> Result<(), LayertwineError> {
    run_cli_sync(_config)
}

/// Run in HTTP mode
#[cfg(feature = "http")]
async fn run_http(config: &CommonConfig, addr: SocketAddr) -> Result<(), LayertwineError> {
    use crate::api::service::ServiceConfig;
    use std::sync::Arc;

    let service = Arc::new(crate::api::service::ApiService::open(ServiceConfig {
        db_path: config.db_path.clone(),
    })?);

    eprintln!("Starting HTTP server on {}", addr);
    crate::api::http::serve(service, addr).await
}

/// Run in gRPC mode
#[cfg(feature = "grpc")]
async fn run_grpc(config: &CommonConfig, addr: SocketAddr) -> Result<(), LayertwineError> {
    use crate::api::service::ServiceConfig;
    use std::sync::Arc;

    let service = Arc::new(crate::api::service::ApiService::open(ServiceConfig {
        db_path: config.db_path.clone(),
    })?);

    eprintln!("Starting gRPC server on {}", addr);
    crate::api::rpc::serve(service, addr).await
}
