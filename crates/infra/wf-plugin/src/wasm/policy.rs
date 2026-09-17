use wf_plugin_sdk::manifest::{
    PluginManifest, PluginPermission, WasmConfig, WASM_DEFAULT_FUEL_LIMIT,
    WASM_DEFAULT_MAX_MODULE_BYTES, WASM_DEFAULT_MEMORY_MAX_MB,
};

use crate::error::{PluginError, PluginResult};

/// Resolved per-call execution limits for one wasm plugin.
#[derive(Debug, Clone)]
pub struct WasmLimits {
    /// Linear-memory cap in bytes.
    pub memory_max_bytes: u64,
    /// Fuel budget per guest call; `None` means effectively unbounded
    /// (implemented as fuel that cannot be exhausted).
    pub fuel_limit: Option<u64>,
    /// Wall-clock timeout per guest call in ms; `None` means no epoch
    /// deadline (the outer `PluginGuard` timeout still applies).
    pub call_timeout_ms: Option<u64>,
    /// Maximum accepted `.wasm` module size in bytes.
    pub max_module_bytes: u64,
    /// Idle guest sessions retained for reuse. `0` disables pooling.
    pub pool_size: usize,
}

/// Upper bound for retained idle sessions per plugin. Each session holds a
/// full linear memory, so the cap keeps a misconfigured pool from eating
/// unbounded memory.
pub const MAX_POOL_SIZE: usize = 64;

/// Resolved WASI capability grants for one wasm plugin.
///
/// Deny-by-default: every field starts empty and is only populated when the
/// manifest both declares the matching permission and lists the grant.
#[derive(Debug, Clone, Default)]
pub struct WasiGrants {
    /// `(host_path, guest_path)` directory preopens. Read-only; directories
    /// that also appear in `writable_dirs` are mounted writable instead.
    pub preopened_dirs: Vec<(String, String)>,
    /// `(host_path, guest_path)` directory preopens with read and write
    /// access. Opt-in per directory: absent means the guest cannot write.
    pub writable_dirs: Vec<(String, String)>,
    /// Environment variables inherited from the host.
    pub env_vars: Vec<(String, String)>,
    /// Whether guest network access is granted. Always false:
    /// `allow_network = true` is rejected at load by
    /// `validate_network_policy`, so this field only exists for
    /// observability in startup logs and never enables sockets.
    pub allow_network: bool,
}

/// Resolve execution limits from the manifest, falling back to engine and
/// SDK defaults. Limits never fail loading: unset values fall back, explicit
/// zeros degrade to a safe reading with a warning (see
/// `resolve_limits_with_defaults`).
pub fn resolve_limits(manifest: &PluginManifest, engine_guard_timeout_ms: u64) -> WasmLimits {
    resolve_limits_with_defaults(manifest, None, engine_guard_timeout_ms)
}

/// Resolve execution limits with built-in < engine-global < manifest
/// priority. `call_timeout_ms: Some(0)` disables the epoch deadline with a
/// warning (the outer guard still applies); `memory_max_mb` and
/// `max_module_bytes` of `Some(0)` fall back to their built-in defaults
/// with a warning because memory has no outer backstop and "unlimited" is
/// not expressible. An over-large `store_pool_size` is clamped to
/// `MAX_POOL_SIZE` with a warning instead of failing the load.
pub fn resolve_limits_with_defaults(
    manifest: &PluginManifest,
    engine_defaults: Option<&WasmConfig>,
    engine_guard_timeout_ms: u64,
) -> WasmLimits {
    let cfg = manifest.wasm.as_ref();
    let memory_mb = cfg
        .and_then(|c| c.memory_max_mb)
        .or_else(|| engine_defaults.and_then(|c| c.memory_max_mb))
        .unwrap_or(WASM_DEFAULT_MEMORY_MAX_MB);
    let memory_mb = match memory_mb {
        0 => {
            tracing::warn!(
                "plugin '{}': wasm.memory_max_mb=0 falls back to the built-in default of {WASM_DEFAULT_MEMORY_MAX_MB}MiB",
                manifest.id
            );
            WASM_DEFAULT_MEMORY_MAX_MB
        }
        mb => mb,
    };
    let max_module_bytes = cfg
        .and_then(|c| c.max_module_bytes)
        .or_else(|| engine_defaults.and_then(|c| c.max_module_bytes))
        .unwrap_or(WASM_DEFAULT_MAX_MODULE_BYTES);
    let max_module_bytes = match max_module_bytes {
        0 => {
            tracing::warn!(
                "plugin '{}': wasm.max_module_bytes=0 falls back to the built-in default of {WASM_DEFAULT_MAX_MODULE_BYTES} bytes",
                manifest.id
            );
            WASM_DEFAULT_MAX_MODULE_BYTES
        }
        n => n,
    };
    let fuel_limit = match cfg
        .and_then(|c| c.fuel_limit)
        .or_else(|| engine_defaults.and_then(|c| c.fuel_limit))
    {
        None => Some(WASM_DEFAULT_FUEL_LIMIT),
        Some(0) => None,
        Some(n) => Some(n),
    };
    let call_timeout_ms = match cfg
        .and_then(|c| c.call_timeout_ms)
        .or_else(|| engine_defaults.and_then(|c| c.call_timeout_ms))
    {
        Some(0) => {
            tracing::warn!(
                "plugin '{}': wasm.call_timeout_ms=0 disables the epoch deadline; the outer guard timeout still applies",
                manifest.id
            );
            None
        }
        explicit @ Some(_) => explicit,
        None => {
            if engine_guard_timeout_ms > 0 {
                Some(engine_guard_timeout_ms)
            } else {
                None
            }
        }
    };
    let pool_size = cfg
        .and_then(|c| c.store_pool_size)
        .or_else(|| engine_defaults.and_then(|c| c.store_pool_size))
        .unwrap_or(0) as usize;
    let pool_size = match pool_size > MAX_POOL_SIZE {
        true => {
            tracing::warn!(
                "plugin '{}': wasm.store_pool_size {pool_size} exceeds the maximum of {MAX_POOL_SIZE}; clamped",
                manifest.id
            );
            MAX_POOL_SIZE
        }
        false => pool_size,
    };
    WasmLimits {
        memory_max_bytes: memory_mb.saturating_mul(1024 * 1024),
        fuel_limit,
        call_timeout_ms,
        max_module_bytes,
        pool_size,
    }
}

/// Reject manifests that request guest network access. The host grants
/// no socket access in this phase (neither WASI p1 sockets nor p2
/// `NetworkPreopen` are wired), so `allow_network = true` fails loudly
/// instead of warning and then silently running without network.
pub fn validate_network_policy(manifest: &PluginManifest) -> PluginResult<()> {
    if manifest.wasm.as_ref().and_then(|c| c.allow_network) == Some(true) {
        return Err(PluginError::InvalidManifest(format!(
            "plugin '{}': wasm.allow_network=true is not supported in this phase; remove the field or set it to false",
            manifest.id
        )));
    }
    Ok(())
}

/// Resolve WASI grants from declared permissions plus explicit grant lists.
/// A grant list without the matching permission grants nothing.
pub fn resolve_grants(manifest: &PluginManifest) -> WasiGrants {
    let mut grants = WasiGrants::default();
    let cfg = match manifest.wasm.as_ref() {
        Some(c) => c,
        None => return grants,
    };
    let has = |p: PluginPermission| manifest.permissions.contains(&p);
    if has(PluginPermission::Filesystem) {
        if let Some(dirs) = cfg.allowed_dirs.as_ref() {
            for dir in dirs {
                grants.preopened_dirs.push((dir.clone(), dir.clone()));
            }
        }
        if let Some(dirs) = cfg.allowed_write_dirs.as_ref() {
            for dir in dirs {
                // Write implies read: a directory granted writable must not
                // also be mounted read-only.
                grants.preopened_dirs.retain(|(host, _)| host != dir);
                grants.writable_dirs.push((dir.clone(), dir.clone()));
            }
        }
    }
    if has(PluginPermission::Environment) {
        if let Some(prefixes) = cfg.allowed_env_prefixes.as_ref() {
            for (key, value) in std::env::vars() {
                if prefixes.iter().any(|p| key.starts_with(p)) {
                    grants.env_vars.push((key, value));
                }
            }
        }
    }
    // Network and shell grants are explicitly denied for wasm guests:
    // `allow_network = true` is rejected at load, and the `shell`
    // permission grants nothing (guests have no shell import and must go
    // through host tool contributions). Log the request so the denial is
    // visible instead of silent.
    if has(PluginPermission::Network) {
        tracing::info!(
            "wasm plugin '{}' declares the network permission, which grants no socket access",
            manifest.id
        );
    }
    if has(PluginPermission::Shell) {
        tracing::info!(
            "wasm plugin '{}' declares the shell permission, which is denied for wasm guests",
            manifest.id
        );
    }
    grants
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_plugin_sdk::manifest::{PluginType, WasmConfig};

    fn manifest_with(
        wasm: Option<WasmConfig>,
        permissions: Vec<PluginPermission>,
    ) -> PluginManifest {
        PluginManifest {
            id: "w".into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            sdk_version: None,
            entry_point: "plugin.wasm".into(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions,
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            wasm,
            lua: None,
        }
    }
    #[test]
    fn limits_fall_back_to_defaults() {
        let limits = resolve_limits(&manifest_with(None, vec![]), 10000);
        assert_eq!(
            limits.memory_max_bytes,
            WASM_DEFAULT_MEMORY_MAX_MB * 1024 * 1024
        );
        assert_eq!(limits.fuel_limit, Some(WASM_DEFAULT_FUEL_LIMIT));
        assert_eq!(limits.call_timeout_ms, Some(10000));
        assert_eq!(limits.max_module_bytes, WASM_DEFAULT_MAX_MODULE_BYTES);
    }

    #[test]
    fn engine_defaults_apply_and_manifest_wins() {
        let engine = WasmConfig {
            memory_max_mb: Some(32),
            call_timeout_ms: Some(500),
            ..Default::default()
        };
        let plain = manifest_with(None, vec![]);
        let limits = resolve_limits_with_defaults(&plain, Some(&engine), 10000);
        assert_eq!(limits.memory_max_bytes, 32 * 1024 * 1024);
        assert_eq!(limits.call_timeout_ms, Some(500));

        let mut with_manifest = manifest_with(
            Some(WasmConfig {
                memory_max_mb: Some(16),
                ..Default::default()
            }),
            vec![],
        );
        let limits = resolve_limits_with_defaults(&with_manifest, Some(&engine), 10000);
        assert_eq!(limits.memory_max_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.call_timeout_ms, Some(500));

        with_manifest.wasm.as_mut().expect("wasm").call_timeout_ms = Some(700);
        let limits = resolve_limits_with_defaults(&with_manifest, Some(&engine), 10000);
        assert_eq!(limits.call_timeout_ms, Some(700));
    }

    #[test]
    fn zero_memory_and_size_fall_back_to_defaults() {
        let m = manifest_with(
            Some(WasmConfig {
                memory_max_mb: Some(0),
                max_module_bytes: Some(0),
                ..Default::default()
            }),
            vec![],
        );
        let limits = resolve_limits(&m, 0);
        assert_eq!(
            limits.memory_max_bytes,
            WASM_DEFAULT_MEMORY_MAX_MB * 1024 * 1024
        );
        assert_eq!(limits.max_module_bytes, WASM_DEFAULT_MAX_MODULE_BYTES);

        let engine = WasmConfig {
            memory_max_mb: Some(0),
            ..Default::default()
        };
        let plain = manifest_with(None, vec![]);
        let limits = resolve_limits_with_defaults(&plain, Some(&engine), 0);
        assert_eq!(
            limits.memory_max_bytes,
            WASM_DEFAULT_MEMORY_MAX_MB * 1024 * 1024
        );
    }

    #[test]
    fn zero_call_timeout_disables_epoch_deadline() {
        let m = manifest_with(
            Some(WasmConfig {
                call_timeout_ms: Some(0),
                ..Default::default()
            }),
            vec![],
        );
        let limits = resolve_limits(&m, 10000);
        assert_eq!(limits.call_timeout_ms, None);
    }

    #[test]
    fn pool_size_defaults_to_disabled_and_clamps_overflow() {
        let m = manifest_with(None, vec![]);
        assert_eq!(resolve_limits(&m, 100).pool_size, 0);

        let m = manifest_with(
            Some(WasmConfig {
                store_pool_size: Some(0),
                ..Default::default()
            }),
            vec![],
        );
        assert_eq!(resolve_limits(&m, 100).pool_size, 0);

        let m = manifest_with(
            Some(WasmConfig {
                store_pool_size: Some(4),
                ..Default::default()
            }),
            vec![],
        );
        assert_eq!(resolve_limits(&m, 100).pool_size, 4);

        let m = manifest_with(
            Some(WasmConfig {
                store_pool_size: Some(MAX_POOL_SIZE as u32 + 1),
                ..Default::default()
            }),
            vec![],
        );
        assert_eq!(resolve_limits(&m, 100).pool_size, MAX_POOL_SIZE);
    }

    #[test]
    fn zero_fuel_disables_metering() {
        let m = manifest_with(
            Some(WasmConfig {
                fuel_limit: Some(0),
                call_timeout_ms: Some(500),
                ..Default::default()
            }),
            vec![],
        );
        let limits = resolve_limits(&m, 10000);
        assert_eq!(limits.fuel_limit, None);
        assert_eq!(limits.call_timeout_ms, Some(500));
    }

    #[test]
    fn network_true_is_rejected_with_or_without_permission() {
        for permissions in [
            vec![PluginPermission::Network],
            vec![],
            vec![PluginPermission::Filesystem],
        ] {
            let m = manifest_with(
                Some(WasmConfig {
                    allow_network: Some(true),
                    ..Default::default()
                }),
                permissions,
            );
            let err = validate_network_policy(&m).expect_err("network must fail");
            assert!(err.to_string().contains("allow_network"), "got: {err}");
        }
        let m = manifest_with(
            Some(WasmConfig {
                allow_network: Some(false),
                ..Default::default()
            }),
            vec![PluginPermission::Network],
        );
        assert!(validate_network_policy(&m).is_ok());
        assert!(validate_network_policy(&manifest_with(None, vec![])).is_ok());
    }

    #[test]
    fn env_prefixes_filter_host_vars() {
        std::env::set_var("WF_WASM_TEST_UNIQ_A", "1");
        std::env::set_var("WF_WASM_TEST_OTHER", "1");
        let cfg = || {
            Some(WasmConfig {
                allowed_env_prefixes: Some(vec!["WF_WASM_TEST_UNIQ_".into()]),
                ..Default::default()
            })
        };
        let granted = resolve_grants(&manifest_with(cfg(), vec![PluginPermission::Environment]));
        assert!(granted
            .env_vars
            .iter()
            .any(|(k, _)| k == "WF_WASM_TEST_UNIQ_A"));
        assert!(!granted
            .env_vars
            .iter()
            .any(|(k, _)| k == "WF_WASM_TEST_OTHER"));
        let denied = resolve_grants(&manifest_with(cfg(), vec![]));
        assert!(denied.env_vars.is_empty());
        std::env::remove_var("WF_WASM_TEST_UNIQ_A");
        std::env::remove_var("WF_WASM_TEST_OTHER");
    }

    #[test]
    fn grants_require_matching_permission() {
        let cfg = || {
            Some(WasmConfig {
                allowed_dirs: Some(vec!["./data".into()]),
                ..Default::default()
            })
        };
        let without_perm = resolve_grants(&manifest_with(cfg(), vec![]));
        assert!(without_perm.preopened_dirs.is_empty());
        let with_perm = resolve_grants(&manifest_with(cfg(), vec![PluginPermission::Filesystem]));
        assert_eq!(with_perm.preopened_dirs.len(), 1);
    }

    #[test]
    fn writable_dirs_require_permission_and_imply_read() {
        let cfg = || {
            Some(WasmConfig {
                allowed_dirs: Some(vec!["./data".into()]),
                allowed_write_dirs: Some(vec!["./data".into(), "./cache".into()]),
                ..Default::default()
            })
        };
        let denied = resolve_grants(&manifest_with(cfg(), vec![]));
        assert!(denied.writable_dirs.is_empty());
        assert!(denied.preopened_dirs.is_empty());

        let granted = resolve_grants(&manifest_with(cfg(), vec![PluginPermission::Filesystem]));
        assert_eq!(granted.writable_dirs.len(), 2);
        // `./data` was requested read-write: it mounts once, writable.
        assert!(granted.preopened_dirs.is_empty());
    }

    #[test]
    fn read_only_dirs_stay_read_only() {
        let cfg = || {
            Some(WasmConfig {
                allowed_dirs: Some(vec!["./ro".into()]),
                ..Default::default()
            })
        };
        let granted = resolve_grants(&manifest_with(cfg(), vec![PluginPermission::Filesystem]));
        assert_eq!(granted.preopened_dirs.len(), 1);
        assert!(granted.writable_dirs.is_empty());
    }
}
