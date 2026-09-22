//! Server-owned configuration: transport plus middleware.
//!
//! The server owns its full runtime surface instead of borrowing it from
//! metrics or infrastructure config. Everything resolves with one explicit
//! priority chain:
//!
//! CLI `--addr` flag > `*_ENABLED` / `API_KEYS` / `WF_SERVER_BIND_ADDR` env >
//! `configs/server/*.toml` file layer > built-in defaults.
//!
//! Secrets never live in files: API keys come from the `API_KEYS`
//! environment variable only. The file layer is lenient: missing files fall
//! back to defaults and present but invalid files are skipped with a
//! warning, so a typo never silently binds an unexpected address or opens
//! the API surface.

use std::net::SocketAddr;
use std::path::Path;

use serde::Deserialize;

use crate::middleware::ServerMiddlewareConfig;

/// Built-in listen address used when no other source provides one.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";

/// Environment variable overriding the listen address.
pub const BIND_ADDR_ENV_VAR: &str = "WF_SERVER_BIND_ADDR";

/// File name of the server transport config inside the server directory
/// (`{project_root}/configs/server/server.toml`).
pub const SERVER_CONFIG_FILE: &str = "server.toml";

/// Owned server configuration: transport plus middleware.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub middleware: ServerMiddlewareConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            middleware: ServerMiddlewareConfig::default(),
        }
    }
}

impl ServerConfig {
    /// Resolve the effective server configuration from CLI flag, environment,
    /// file layer and defaults, in that priority order.
    pub fn resolve(project_root: Option<&Path>, cli_addr: Option<SocketAddr>) -> Self {
        let server_dir = project_root
            .map(|root| wf_config::layout::family_dir(root, wf_config::layout::family::SERVER));

        let config = Self {
            bind_addr: Self::resolve_bind_addr(server_dir.as_deref(), cli_addr),
            middleware: ServerMiddlewareConfig::resolve(server_dir.as_deref()),
        };
        config
    }

    fn resolve_bind_addr(server_dir: Option<&Path>, cli_addr: Option<SocketAddr>) -> SocketAddr {
        if let Some(addr) = cli_addr {
            return addr;
        }
        if let Ok(raw) = std::env::var(BIND_ADDR_ENV_VAR) {
            match raw.parse::<SocketAddr>() {
                Ok(addr) => return addr,
                Err(e) => {
                    tracing::warn!(error = %e, value = %raw, "invalid WF_SERVER_BIND_ADDR; ignoring");
                }
            }
        }
        if let Some(dir) = server_dir {
            if let Some(addr) = load_bind_addr_from_file(&dir.join(SERVER_CONFIG_FILE)) {
                return addr;
            }
        }
        default_bind_addr()
    }
}

fn default_bind_addr() -> SocketAddr {
    DEFAULT_BIND_ADDR
        .parse()
        .expect("built-in default bind address parses")
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ServerFileConfig {
    bind_addr: Option<String>,
}

fn load_bind_addr_from_file(path: &Path) -> Option<SocketAddr> {
    if !path.exists() {
        return None;
    }
    let config: ServerFileConfig = match wf_config::layered::load_layered_config_sync(&[path]) {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "invalid server config file; ignoring");
            return None;
        }
    };
    match config.bind_addr {
        None => None,
        Some(raw) => match raw.parse::<SocketAddr>() {
            Ok(addr) => Some(addr),
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "invalid bind_addr in server config file; ignoring");
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_dir(root: &Path) -> std::path::PathBuf {
        root.join("configs").join("server")
    }

    #[test]
    fn default_bind_addr_parses() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_addr.to_string(), DEFAULT_BIND_ADDR);
    }

    #[test]
    fn cli_addr_wins_over_everything() {
        let cli: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = ServerConfig::resolve(None, Some(cli));
        assert_eq!(config.bind_addr, cli);
    }

    #[test]
    fn file_layer_provides_bind_addr() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(server_dir(dir.path())).unwrap();
        std::fs::write(
            server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"127.0.0.1:4000\"\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None);
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4000");
    }

    #[test]
    fn invalid_file_falls_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(server_dir(dir.path())).unwrap();
        std::fs::write(
            server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"not-an-addr\"\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None);
        assert_eq!(config.bind_addr.to_string(), DEFAULT_BIND_ADDR);
    }

    #[test]
    fn middleware_file_layer_applies() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(server_dir(dir.path())).unwrap();
        std::fs::write(
            server_dir(dir.path()).join(crate::middleware::CORS_CONFIG_FILE),
            "allowed_origins = [\"https://app.example.com\"]\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None);
        assert_eq!(
            config.middleware.cors.allowed_origins,
            vec!["https://app.example.com".to_string()]
        );
    }
}
