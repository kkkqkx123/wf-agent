//! Server-owned configuration: transport plus middleware.
//!
//! The server owns its full runtime surface instead of borrowing it from
//! metrics or infrastructure config. Everything resolves with one explicit
//! priority chain:
//!
//! CLI `--addr` flag > `*_ENABLED` / `API_KEYS` / `WF_SERVER_BIND_ADDR` env >
//! project `.wf/server/*.toml` > `{settings_dir}/server/*.toml` > built-in
//! defaults.
//!
//! Secrets never live in files: API keys come from the `API_KEYS`
//! environment variable only. Missing files fall back to defaults; a
//! present but invalid file fails startup, matching the infrastructure
//! file layer so a typo cannot silently bind an unexpected address or
//! open the API surface.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use wf_config::error::{ConfigError, ConfigResult};

use crate::middleware::ServerMiddlewareConfig;

/// Built-in listen address used when no other source provides one.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";

/// Environment variable overriding the listen address.
pub const BIND_ADDR_ENV_VAR: &str = "WF_SERVER_BIND_ADDR";

/// Environment variable pointing at the web frontend build to serve.
pub const STATIC_DIR_ENV_VAR: &str = "WF_SERVER_STATIC_DIR";

/// File name of the server transport config inside each server layer
/// directory (`server.toml`).
pub const SERVER_CONFIG_FILE: &str = "server.toml";

/// Owned server configuration: transport plus middleware.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub middleware: ServerMiddlewareConfig,
    /// Web frontend build directory served with SPA fallback; `None`
    /// disables static hosting (API-only mode).
    pub static_dir: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            middleware: ServerMiddlewareConfig::default(),
            static_dir: None,
        }
    }
}

impl ServerConfig {
    /// Resolve the effective server configuration from CLI flag, environment,
    /// file layer and defaults, in that priority order. The file layer merges
    /// the settings layer (low) and the project `.wf/server/` layer (high).
    pub fn resolve(
        project_root: Option<&Path>,
        settings_dir: Option<&Path>,
        cli_addr: Option<SocketAddr>,
        cli_static_dir: Option<PathBuf>,
    ) -> ConfigResult<Self> {
        let project_server_dir;
        let settings_server_dir;
        let project_layer = match project_root {
            Some(root) => {
                project_server_dir = wf_config::layout::project_server_dir(root);
                Some(project_server_dir.as_path())
            }
            None => None,
        };
        let settings_layer = match settings_dir {
            Some(dir) => {
                settings_server_dir = wf_config::layout::settings_server_dir(dir);
                Some(settings_server_dir.as_path())
            }
            None => None,
        };
        Ok(Self {
            bind_addr: Self::resolve_bind_addr(settings_layer, project_layer, cli_addr)?,
            middleware: ServerMiddlewareConfig::resolve(settings_layer, project_layer)?,
            static_dir: Self::resolve_static_dir(settings_layer, project_layer, cli_static_dir)?,
        })
    }

    fn resolve_bind_addr(
        settings_layer: Option<&Path>,
        project_layer: Option<&Path>,
        cli_addr: Option<SocketAddr>,
    ) -> ConfigResult<SocketAddr> {
        if let Some(addr) = cli_addr {
            return Ok(addr);
        }
        if let Ok(raw) = std::env::var(BIND_ADDR_ENV_VAR) {
            match raw.parse::<SocketAddr>() {
                Ok(addr) => return Ok(addr),
                Err(e) => {
                    tracing::warn!(error = %e, value = %raw, "invalid WF_SERVER_BIND_ADDR; ignoring");
                }
            }
        }
        if let Some(file) = load_merged_server_file(settings_layer, project_layer)? {
            if let Some(raw) = file.bind_addr {
                return raw.parse::<SocketAddr>().map_err(|e| {
                    ConfigError::Parse(format!("invalid bind_addr in server config: {e}"))
                });
            }
        }
        Ok(default_bind_addr())
    }

    fn resolve_static_dir(
        settings_layer: Option<&Path>,
        project_layer: Option<&Path>,
        cli_static_dir: Option<PathBuf>,
    ) -> ConfigResult<Option<PathBuf>> {
        if let Some(dir) = cli_static_dir {
            return Ok(Some(dir));
        }
        if let Ok(raw) = std::env::var(STATIC_DIR_ENV_VAR) {
            if !raw.trim().is_empty() {
                return Ok(Some(PathBuf::from(raw)));
            }
        }
        if let Some(file) = load_merged_server_file(settings_layer, project_layer)? {
            if let Some(raw) = file.static_dir.filter(|raw| !raw.trim().is_empty()) {
                return Ok(Some(PathBuf::from(raw)));
            }
        }
        Ok(None)
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
    static_dir: Option<String>,
}

/// Load and merge `server.toml` from the settings layer (low) and the
/// project layer (high). Missing files fall back to `None`; a present but
/// invalid file fails.
fn load_merged_server_file(
    settings_layer: Option<&Path>,
    project_layer: Option<&Path>,
) -> ConfigResult<Option<ServerFileConfig>> {
    let mut paths = Vec::new();
    if let Some(dir) = settings_layer {
        paths.push(dir.join(SERVER_CONFIG_FILE));
    }
    if let Some(dir) = project_layer {
        paths.push(dir.join(SERVER_CONFIG_FILE));
    }
    let existing: Vec<&Path> = paths
        .iter()
        .map(|path| path.as_path())
        .filter(|path| path.exists())
        .collect();
    if existing.is_empty() {
        return Ok(None);
    }
    wf_config::layered::load_layered_config_sync::<ServerFileConfig>(&existing)
        .map(Some)
        .map_err(|e| {
            ConfigError::Parse(format!(
                "invalid server config files {}: {e}",
                existing
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_server_dir(root: &Path) -> std::path::PathBuf {
        root.join(".wf").join("server")
    }

    fn settings_server_dir(root: &Path) -> std::path::PathBuf {
        root.join("server")
    }

    #[test]
    fn default_bind_addr_parses() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_addr.to_string(), DEFAULT_BIND_ADDR);
    }

    #[test]
    fn cli_addr_wins_over_everything() {
        let cli: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = ServerConfig::resolve(None, None, Some(cli), None).unwrap();
        assert_eq!(config.bind_addr, cli);
    }

    #[test]
    fn file_layer_provides_bind_addr() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"127.0.0.1:4000\"\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4000");
    }

    #[test]
    fn project_layer_wins_over_settings_layer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(settings_server_dir(dir.path())).unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            settings_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"127.0.0.1:4000\"\n",
        )
        .unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"127.0.0.1:4001\"\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), Some(dir.path()), None, None).unwrap();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4001");
    }

    #[test]
    fn settings_layer_fills_without_project_layer() {
        let settings = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(settings_server_dir(settings.path())).unwrap();
        std::fs::write(
            settings_server_dir(settings.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"127.0.0.1:4002\"\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(None, Some(settings.path()), None, None).unwrap();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4002");
    }

    #[test]
    fn invalid_file_fails_startup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "bind_addr = \"not-an-addr\"\n",
        )
        .unwrap();
        let err = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap_err();
        assert!(err.to_string().contains("invalid bind_addr"));
    }

    #[test]
    fn middleware_file_layer_applies() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(crate::middleware::CORS_CONFIG_FILE),
            "allowed_origins = [\"https://app.example.com\"]\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        assert_eq!(
            config.middleware.cors.allowed_origins,
            vec!["https://app.example.com".to_string()]
        );
    }

    #[test]
    fn static_dir_defaults_to_none_and_cli_wins() {
        let config = ServerConfig::resolve(None, None, None, None).unwrap();
        assert_eq!(config.static_dir, None);
        let config =
            ServerConfig::resolve(None, None, None, Some(PathBuf::from("web/dist"))).unwrap();
        assert_eq!(config.static_dir, Some(PathBuf::from("web/dist")));
    }

    #[test]
    fn file_layer_provides_static_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "static_dir = \"web/dist\"\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        assert_eq!(config.static_dir, Some(PathBuf::from("web/dist")));
    }
}
