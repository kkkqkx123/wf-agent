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
use wf_config::env::{env_parse_bool, EnvValue};
use wf_config::error::{ConfigError, ConfigResult};

use crate::backends::BackendConfig;
use crate::directory::DirectoryConfig;
use crate::middleware::ServerMiddlewareConfig;

/// Built-in listen address used when no other source provides one.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";

/// Environment variable overriding the listen address.
pub const BIND_ADDR_ENV_VAR: &str = "WF_SERVER_BIND_ADDR";

/// Environment variable pointing at the web frontend build to serve.
pub const STATIC_DIR_ENV_VAR: &str = "WF_SERVER_STATIC_DIR";
/// Environment variable overriding the SPA fallback page name.
pub const STATIC_INDEX_ENV_VAR: &str = "WF_SERVER_STATIC_INDEX";
/// Environment variable toggling the SPA fallback for extension-less paths.
pub const STATIC_FALLBACK_ENV_VAR: &str = "WF_SERVER_STATIC_FALLBACK";
/// Environment variable overriding the cache lifetime of immutable assets.
pub const STATIC_IMMUTABLE_MAX_AGE_ENV_VAR: &str = "WF_SERVER_STATIC_IMMUTABLE_MAX_AGE_SECS";

/// Default behavior when an SSE observer disconnects from a streaming run.
pub const DEFAULT_STREAM_DETACH_POLICY: &str = "cancel";
/// Default idle threshold for reclaiming a detached run (seconds, 30 min).
pub const DEFAULT_RUN_DETACH_IDLE_SECS: u64 = 1800;
/// Default per-principal cap on live detached runs.
pub const DEFAULT_RUN_DETACH_MAX_PER_PRINCIPAL: usize = 4;
/// Lower bound for the detached-run idle threshold (5 min).
pub const MIN_RUN_DETACH_IDLE_SECS: u64 = 300;
/// Upper bound for the detached-run idle threshold (24 h).
pub const MAX_RUN_DETACH_IDLE_SECS: u64 = 86_400;

/// Environment variable overriding the detach policy.
pub const STREAM_DETACH_POLICY_ENV_VAR: &str = "WF_SERVER_STREAM_DETACH_POLICY";
/// Environment variable overriding the detached-run idle threshold.
pub const RUN_DETACH_IDLE_SECS_ENV_VAR: &str = "WF_SERVER_RUN_DETACH_IDLE_SECS";
/// Environment variable overriding the per-principal detached cap.
pub const RUN_DETACH_MAX_PER_PRINCIPAL_ENV_VAR: &str = "WF_SERVER_RUN_DETACH_MAX_PER_PRINCIPAL";

/// Default ceiling for graceful HTTP drain during shutdown (seconds).
pub const DEFAULT_SHUTDOWN_DRAIN_SECS: u64 = 10;
/// Environment variable overriding the graceful drain budget.
pub const SHUTDOWN_DRAIN_SECS_ENV_VAR: &str = "WF_SERVER_SHUTDOWN_DRAIN_SECS";
/// Lower bound for the drain budget: zero means force-close immediately.
pub const MIN_SHUTDOWN_DRAIN_SECS: u64 = 0;
/// Upper bound for the drain budget, so a typo cannot hang shutdown forever.
pub const MAX_SHUTDOWN_DRAIN_SECS: u64 = 120;

/// Default SPA fallback page served for extension-less app routes.
pub const DEFAULT_STATIC_INDEX: &str = "index.html";
/// Default cache lifetime for immutable fingerprinted assets (one year).
pub const DEFAULT_STATIC_IMMUTABLE_MAX_AGE_SECS: u64 = 31_536_000;

/// File name of the server transport config inside each server layer
/// directory (`server.toml`).
pub const SERVER_CONFIG_FILE: &str = "server.toml";

/// Owned server configuration: transport plus middleware plus the
/// multi-directory allowlist.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub middleware: ServerMiddlewareConfig,
    /// Web frontend hosting: `None` disables static hosting (API-only
    /// mode); `Some` serves the build directory with SPA fallback on the
    /// same port. Relative directories resolve against the server
    /// executable so a single-host bundle works from any workdir.
    pub static_config: Option<StaticConfig>,
    /// Directory allowlist for multi-directory serving. Empty permits every
    /// validated explicit directory; a non-empty list restricts serving to
    /// the listed roots.
    pub directory: DirectoryConfig,
    /// Backend management domain (registry hosting, probing, tunnels,
    /// backups). Disabled by default; see `backends.toml`.
    pub backends: BackendConfig,
    /// Maximum seconds to drain in-flight HTTP requests on shutdown before
    /// force-closing remaining connections. Bounds the wait on SSE clients.
    pub shutdown_drain_secs: u64,
    /// Detached-stream lifecycle policy and quotas.
    pub stream_lifecycle: StreamLifecycleConfig,
}

/// Resolved keepalive knobs shared by the streaming and dispatch handlers.
#[derive(Debug, Clone)]
pub struct StreamLifecycleConfig {
    /// What happens to a run when its SSE observer disconnects.
    pub detach_policy: wf_api::agent::agent_execution::StreamDetachPolicy,
    /// Reclaim a detached run after this long with a last event and no
    /// subscriber. Persistent runs are exempt.
    pub detach_idle_secs: u64,
    /// Maximum number of live detached runs per principal.
    pub detach_max_per_principal: usize,
}

impl Default for StreamLifecycleConfig {
    fn default() -> Self {
        Self {
            detach_policy: wf_api::agent::agent_execution::StreamDetachPolicy::Cancel,
            detach_idle_secs: DEFAULT_RUN_DETACH_IDLE_SECS,
            detach_max_per_principal: DEFAULT_RUN_DETACH_MAX_PER_PRINCIPAL,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            middleware: ServerMiddlewareConfig::default(),
            static_config: None,
            directory: DirectoryConfig::default(),
            backends: BackendConfig::default(),
            shutdown_drain_secs: DEFAULT_SHUTDOWN_DRAIN_SECS,
            stream_lifecycle: StreamLifecycleConfig::default(),
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
        let file = load_merged_server_file(settings_layer, project_layer)?;
        Ok(Self {
            bind_addr: Self::resolve_bind_addr(cli_addr, file.as_ref())?,
            middleware: ServerMiddlewareConfig::resolve(settings_layer, project_layer)?,
            static_config: Self::resolve_static(cli_static_dir, file.as_ref())?,
            directory: Self::resolve_directory(file.as_ref())?,
            backends: BackendConfig::resolve(settings_layer, project_layer)?,
            shutdown_drain_secs: Self::resolve_shutdown_drain(file.as_ref())?,
            stream_lifecycle: Self::resolve_stream_lifecycle(file.as_ref())?,
        })
    }

    /// Resolve the keepalive knobs: environment overrides the file layer,
    /// which overrides defaults. A present but invalid value fails startup so
    /// a typo can never silently change the runtime detach behavior.
    fn resolve_stream_lifecycle(
        file: Option<&ServerFileConfig>,
    ) -> ConfigResult<StreamLifecycleConfig> {
        let policy_raw = std::env::var(STREAM_DETACH_POLICY_ENV_VAR)
            .ok()
            .filter(|raw| !raw.trim().is_empty())
            .or_else(|| {
                file.and_then(|file| file.stream_detach_policy.clone())
                    .filter(|raw| !raw.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_STREAM_DETACH_POLICY.to_string());
        let detach_policy = wf_api::agent::agent_execution::StreamDetachPolicy::parse(&policy_raw)
            .ok_or_else(|| {
                ConfigError::Parse(format!(
                    "{STREAM_DETACH_POLICY_ENV_VAR} must be 'cancel' or 'keep', got '{policy_raw}'"
                ))
            })?;

        let detach_idle_secs = resolve_u64_knob(
            RUN_DETACH_IDLE_SECS_ENV_VAR,
            file.and_then(|file| file.run_detach_idle_secs),
            DEFAULT_RUN_DETACH_IDLE_SECS,
            Some(MIN_RUN_DETACH_IDLE_SECS),
            Some(MAX_RUN_DETACH_IDLE_SECS),
        )?;

        let detach_max_per_principal = resolve_u64_knob(
            RUN_DETACH_MAX_PER_PRINCIPAL_ENV_VAR,
            file.and_then(|file| file.run_detach_max_per_principal),
            DEFAULT_RUN_DETACH_MAX_PER_PRINCIPAL as u64,
            Some(1),
            Some(64),
        )? as usize;

        Ok(StreamLifecycleConfig {
            detach_policy,
            detach_idle_secs,
            detach_max_per_principal,
        })
    }

    /// Resolve the graceful-drain budget: environment overrides the file
    /// layer, which overrides the built-in default. A present but invalid
    /// value fails startup rather than silently falling back to an unbounded
    /// wait.
    fn resolve_shutdown_drain(file: Option<&ServerFileConfig>) -> ConfigResult<u64> {
        let raw = std::env::var(SHUTDOWN_DRAIN_SECS_ENV_VAR)
            .ok()
            .filter(|raw| !raw.trim().is_empty())
            .or_else(|| file.and_then(|file| file.shutdown_drain_secs.map(|v| v.to_string())));
        let Some(raw) = raw else {
            return Ok(DEFAULT_SHUTDOWN_DRAIN_SECS);
        };
        let secs = raw.trim().parse::<u64>().map_err(|e| {
            ConfigError::Parse(format!("invalid {SHUTDOWN_DRAIN_SECS_ENV_VAR}: {e}"))
        })?;
        if !(MIN_SHUTDOWN_DRAIN_SECS..=MAX_SHUTDOWN_DRAIN_SECS).contains(&secs) {
            return Err(ConfigError::Parse(format!(
                "{SHUTDOWN_DRAIN_SECS_ENV_VAR} must be between {MIN_SHUTDOWN_DRAIN_SECS} and {MAX_SHUTDOWN_DRAIN_SECS}, got {secs}"
            )));
        }
        Ok(secs)
    }

    fn resolve_bind_addr(
        cli_addr: Option<SocketAddr>,
        file: Option<&ServerFileConfig>,
    ) -> ConfigResult<SocketAddr> {
        if let Some(addr) = cli_addr {
            return Ok(addr);
        }
        if let Ok(raw) = std::env::var(BIND_ADDR_ENV_VAR) {
            if !raw.trim().is_empty() {
                return raw
                    .trim()
                    .parse::<SocketAddr>()
                    .map_err(|e| ConfigError::Parse(format!("invalid {BIND_ADDR_ENV_VAR}: {e}")));
            }
        }
        if let Some(raw) = file.and_then(|file| file.bind_addr.as_deref()) {
            return raw.parse::<SocketAddr>().map_err(|e| {
                ConfigError::Parse(format!("invalid bind_addr in server config: {e}"))
            });
        }
        Ok(default_bind_addr())
    }

    fn resolve_static(
        cli_static_dir: Option<PathBuf>,
        file: Option<&ServerFileConfig>,
    ) -> ConfigResult<Option<StaticConfig>> {
        let raw_dir = cli_static_dir
            .map(|dir| dir.display().to_string())
            .or_else(|| {
                std::env::var(STATIC_DIR_ENV_VAR)
                    .ok()
                    .filter(|raw| !raw.trim().is_empty())
                    .or_else(|| {
                        file.and_then(|file| file.static_dir.clone())
                            .filter(|raw| !raw.trim().is_empty())
                    })
            });
        let Some(raw_dir) = raw_dir else {
            return Ok(None);
        };
        let index = std::env::var(STATIC_INDEX_ENV_VAR)
            .ok()
            .filter(|raw| !raw.trim().is_empty())
            .or_else(|| {
                file.and_then(|file| file.static_index.clone())
                    .filter(|raw| !raw.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_STATIC_INDEX.to_string());
        let fallback = match std::env::var(STATIC_FALLBACK_ENV_VAR)
            .ok()
            .filter(|raw| !raw.trim().is_empty())
        {
            Some(raw) => match env_parse_bool(&raw) {
                Ok(EnvValue::Bool(value)) => value,
                Ok(_) => {
                    return Err(ConfigError::Parse(format!(
                        "invalid {STATIC_FALLBACK_ENV_VAR}"
                    )))
                }
                Err(e) => {
                    return Err(ConfigError::Parse(format!(
                        "invalid {STATIC_FALLBACK_ENV_VAR}: {e}"
                    )))
                }
            },
            None => file.and_then(|file| file.static_fallback).unwrap_or(true),
        };
        let immutable_max_age_secs = match std::env::var(STATIC_IMMUTABLE_MAX_AGE_ENV_VAR)
            .ok()
            .filter(|raw| !raw.trim().is_empty())
        {
            Some(raw) => raw.trim().parse::<u64>().map_err(|_| {
                ConfigError::Parse(format!(
                    "invalid {STATIC_IMMUTABLE_MAX_AGE_ENV_VAR}: expected seconds as u64"
                ))
            })?,
            None => file
                .and_then(|file| file.static_immutable_max_age_secs)
                .unwrap_or(DEFAULT_STATIC_IMMUTABLE_MAX_AGE_SECS),
        };
        StaticConfig::resolve_validated(raw_dir, index, fallback, immutable_max_age_secs).map(Some)
    }

    /// Resolve the directory allowlist: project file layer wins over the
    /// settings file layer, then the environment wins over both. A present
    /// but invalid file fails startup so a typo cannot silently close off
    /// served directories.
    fn resolve_directory(file: Option<&ServerFileConfig>) -> ConfigResult<DirectoryConfig> {
        let (allowed, trusted) = match file {
            Some(file) => (file.allowed_roots.clone(), file.trusted_roots.clone()),
            None => (None, None),
        };
        Ok(DirectoryConfig::resolve(allowed, trusted))
    }
}

fn default_bind_addr() -> SocketAddr {
    DEFAULT_BIND_ADDR
        .parse()
        .expect("built-in default bind address parses")
}

/// Resolve one numeric knob: env > file > default, with inclusive bounds.
/// A present but invalid value fails startup instead of silently falling back.
fn resolve_u64_knob(
    env_name: &str,
    file_value: Option<u64>,
    default: u64,
    min: Option<u64>,
    max: Option<u64>,
) -> ConfigResult<u64> {
    let raw = std::env::var(env_name)
        .ok()
        .filter(|raw| !raw.trim().is_empty())
        .or_else(|| file_value.map(|value| value.to_string()));
    let Some(raw) = raw else {
        return Ok(default);
    };
    let value = raw
        .trim()
        .parse::<u64>()
        .map_err(|e| ConfigError::Parse(format!("invalid {env_name}: {e}")))?;
    if min.is_some_and(|low| value < low) || max.is_some_and(|high| value > high) {
        return Err(ConfigError::Parse(format!(
            "{env_name} out of range: got {value}, allowed {}..={}",
            min.map(|v| v.to_string()).unwrap_or_else(|| "0".into()),
            max.map(|v| v.to_string())
                .unwrap_or_else(|| "u64::MAX".into())
        )));
    }
    Ok(value)
}

/// Resolved static hosting: the directory is absolute and validated,
/// so serving never starts against a half-broken page.
#[derive(Debug, Clone)]
pub struct StaticConfig {
    /// Absolute build directory to serve.
    pub dir: PathBuf,
    /// SPA fallback page name inside `dir`.
    pub index: String,
    /// Whether extension-less paths fall back to the index page.
    pub fallback: bool,
    /// Cache lifetime for immutable fingerprinted assets.
    pub immutable_max_age_secs: u64,
}

impl StaticConfig {
    /// Resolve a raw directory against the server executable and
    /// validate it for serving. A relative directory anchors at the
    /// executable's parent so the single-host bundle (`wf-server` next
    /// to `web/`) starts from any working directory; an unavailable
    /// executable path falls back to the working directory.
    pub(crate) fn resolve_validated(
        raw_dir: String,
        index: String,
        fallback: bool,
        immutable_max_age_secs: u64,
    ) -> ConfigResult<Self> {
        let index = index.trim().to_string();
        if index.is_empty() {
            return Err(ConfigError::Parse(
                "static index must not be empty when static hosting is enabled".to_string(),
            ));
        }
        let raw = PathBuf::from(raw_dir.trim());
        if raw.as_os_str().is_empty() {
            return Err(ConfigError::Parse(
                "static dir must not be empty when static hosting is enabled".to_string(),
            ));
        }
        let dir = if raw.is_absolute() {
            raw
        } else {
            let base = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()))
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            base.join(raw)
        };
        if !dir.is_dir() {
            return Err(ConfigError::Parse(format!(
                "static dir is not usable: {}",
                dir.display()
            )));
        }
        if fallback && !dir.join(&index).is_file() {
            return Err(ConfigError::Parse(format!(
                "static fallback page '{index}' is missing in {}",
                dir.display()
            )));
        }
        Ok(Self {
            dir,
            index,
            fallback,
            immutable_max_age_secs,
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ServerFileConfig {
    bind_addr: Option<String>,
    static_dir: Option<String>,
    static_index: Option<String>,
    static_fallback: Option<bool>,
    static_immutable_max_age_secs: Option<u64>,
    allowed_roots: Option<Vec<String>>,
    trusted_roots: Option<Vec<String>>,
    shutdown_drain_secs: Option<u64>,
    stream_detach_policy: Option<String>,
    run_detach_idle_secs: Option<u64>,
    run_detach_max_per_principal: Option<u64>,
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
    wf_config::layered::load_optional_merged_config(&paths, "server")
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
    fn directory_defaults_to_empty_allowlist() {
        let config = ServerConfig::default();
        assert!(config.directory.allowed_roots.is_empty());
    }

    #[test]
    fn file_layer_provides_allowed_roots() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "allowed_roots = [\"/srv/wf/projects\"]\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        assert_eq!(
            config.directory.allowed_roots,
            vec![PathBuf::from("/srv/wf/projects")]
        );
    }

    #[test]
    fn cli_addr_wins_over_everything() {
        let cli: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = ServerConfig::resolve(None, None, Some(cli), None).unwrap();
        assert_eq!(config.bind_addr, cli);
    }

    #[test]
    fn invalid_bind_addr_env_fails_startup() {
        std::env::set_var(BIND_ADDR_ENV_VAR, "not-an-addr");
        let result = ServerConfig::resolve(None, None, None, None);
        std::env::remove_var(BIND_ADDR_ENV_VAR);
        let err = result.unwrap_err();
        assert!(err.to_string().contains(BIND_ADDR_ENV_VAR));
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
    fn static_defaults_to_disabled_and_cli_wins() {
        let config = ServerConfig::resolve(None, None, None, None).unwrap();
        assert!(config.static_config.is_none());
        let site = tempfile::tempdir().unwrap();
        std::fs::write(site.path().join("index.html"), "<app/>").unwrap();
        let config =
            ServerConfig::resolve(None, None, None, Some(site.path().to_path_buf())).unwrap();
        let statik = config.static_config.expect("cli dir enables static");
        assert_eq!(statik.dir, site.path());
        assert_eq!(statik.index, "index.html");
        assert!(statik.fallback);
    }

    #[test]
    fn file_layer_provides_static_dir() {
        let site = tempfile::tempdir().unwrap();
        std::fs::write(site.path().join("index.html"), "<app/>").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            format!("static_dir = \"{}\"\n", site.path().display()),
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        let statik = config.static_config.expect("file dir enables static");
        assert_eq!(statik.dir, site.path());
    }

    #[test]
    fn file_layer_provides_static_tuning() {
        let site = tempfile::tempdir().unwrap();
        std::fs::write(site.path().join("app.html"), "<app/>").unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            format!(
                "static_dir = \"{}\"\nstatic_index = \"app.html\"\nstatic_fallback = false\nstatic_immutable_max_age_secs = 60\n",
                site.path().display()
            ),
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        let statik = config.static_config.expect("file dir enables static");
        assert_eq!(statik.index, "app.html");
        assert!(!statik.fallback);
        assert_eq!(statik.immutable_max_age_secs, 60);
    }

    #[test]
    fn missing_static_dir_fails_startup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "static_dir = \"/nonexistent/wf-web\"\n",
        )
        .unwrap();
        let err = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap_err();
        assert!(err.to_string().contains("static dir is not usable"));
    }

    #[test]
    fn missing_fallback_page_fails_startup() {
        let site = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            format!("static_dir = \"{}\"\n", site.path().display()),
        )
        .unwrap();
        let err = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap_err();
        assert!(err.to_string().contains("fallback page"));
    }

    #[test]
    fn shutdown_drain_defaults_to_ten_secs() {
        let config = ServerConfig::default();
        assert_eq!(config.shutdown_drain_secs, DEFAULT_SHUTDOWN_DRAIN_SECS);
        assert_eq!(DEFAULT_SHUTDOWN_DRAIN_SECS, 10);
    }

    #[test]
    fn file_layer_provides_shutdown_drain() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "shutdown_drain_secs = 25\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        assert_eq!(config.shutdown_drain_secs, 25);
    }

    #[test]
    fn shutdown_drain_out_of_range_fails_startup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "shutdown_drain_secs = 121\n",
        )
        .unwrap();
        let err = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap_err();
        assert!(err.to_string().contains(SHUTDOWN_DRAIN_SECS_ENV_VAR));
    }

    #[test]
    fn stream_lifecycle_defaults() {
        let config = ServerConfig::default();
        assert_eq!(
            config.stream_lifecycle.detach_policy,
            wf_api::agent::agent_execution::StreamDetachPolicy::Cancel
        );
        assert_eq!(
            config.stream_lifecycle.detach_idle_secs,
            DEFAULT_RUN_DETACH_IDLE_SECS
        );
        assert_eq!(
            config.stream_lifecycle.detach_max_per_principal,
            DEFAULT_RUN_DETACH_MAX_PER_PRINCIPAL
        );
    }

    #[test]
    fn file_layer_provides_stream_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "stream_detach_policy = \"keep\"\nrun_detach_idle_secs = 600\nrun_detach_max_per_principal = 8\n",
        )
        .unwrap();
        let config = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap();
        assert_eq!(
            config.stream_lifecycle.detach_policy,
            wf_api::agent::agent_execution::StreamDetachPolicy::Keep
        );
        assert_eq!(config.stream_lifecycle.detach_idle_secs, 600);
        assert_eq!(config.stream_lifecycle.detach_max_per_principal, 8);
    }

    #[test]
    fn invalid_detach_policy_fails_startup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "stream_detach_policy = \"pause\"\n",
        )
        .unwrap();
        let err = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap_err();
        assert!(err.to_string().contains(STREAM_DETACH_POLICY_ENV_VAR));
    }

    #[test]
    fn detach_idle_out_of_range_fails_startup() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project_server_dir(dir.path())).unwrap();
        std::fs::write(
            project_server_dir(dir.path()).join(SERVER_CONFIG_FILE),
            "run_detach_idle_secs = 10\n",
        )
        .unwrap();
        let err = ServerConfig::resolve(Some(dir.path()), None, None, None).unwrap_err();
        assert!(err.to_string().contains(RUN_DETACH_IDLE_SECS_ENV_VAR));
    }
}
