use wf_plugin_sdk::manifest::{
    PluginManifest, PluginPermission, WASM_DEFAULT_FUEL_LIMIT, WASM_DEFAULT_MAX_MODULE_BYTES,
    WASM_DEFAULT_MEMORY_MAX_MB,
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
    /// `(host_path, guest_path)` directory preopens. Read-only in this
    /// phase; writable preopens are a later enhancement.
    pub preopened_dirs: Vec<(String, String)>,
    /// Environment variables inherited from the host.
    pub env_vars: Vec<(String, String)>,
    /// Whether guest network access is granted. Always false in this phase:
    /// WASI p1 has no socket support wired, so the flag is recorded for
    /// policy review but never enforced as an allowance.
    pub allow_network: bool,
}

/// Resolve execution limits from the manifest, falling back to engine and
/// SDK defaults. Rejects explicit zero values that would silently disable
/// protections or make loading impossible.
pub fn resolve_limits(
    manifest: &PluginManifest,
    engine_guard_timeout_ms: u64,
) -> PluginResult<WasmLimits> {
    let cfg = manifest.wasm.as_ref();
    let memory_mb = cfg
        .and_then(|c| c.memory_max_mb)
        .unwrap_or(WASM_DEFAULT_MEMORY_MAX_MB);
    if memory_mb == 0 {
        return Err(PluginError::InvalidManifest(format!(
            "plugin '{}': wasm.memory_max_mb must be > 0",
            manifest.id
        )));
    }
    let max_module_bytes = cfg
        .and_then(|c| c.max_module_bytes)
        .unwrap_or(WASM_DEFAULT_MAX_MODULE_BYTES);
    if max_module_bytes == 0 {
        return Err(PluginError::InvalidManifest(format!(
            "plugin '{}': wasm.max_module_bytes must be > 0",
            manifest.id
        )));
    }
    let fuel_limit = match cfg.and_then(|c| c.fuel_limit) {
        None => Some(WASM_DEFAULT_FUEL_LIMIT),
        Some(0) => None,
        Some(n) => Some(n),
    };
    let call_timeout_ms = cfg
        .and_then(|c| c.call_timeout_ms)
        .or(if engine_guard_timeout_ms > 0 {
            Some(engine_guard_timeout_ms)
        } else {
            None
        });
    let pool_size = cfg.and_then(|c| c.store_pool_size).unwrap_or(0) as usize;
    if pool_size > MAX_POOL_SIZE {
        return Err(PluginError::InvalidManifest(format!(
            "plugin '{}': wasm.store_pool_size {pool_size} exceeds the maximum of {MAX_POOL_SIZE}",
            manifest.id
        )));
    }
    Ok(WasmLimits {
        memory_max_bytes: memory_mb.saturating_mul(1024 * 1024),
        fuel_limit,
        call_timeout_ms,
        max_module_bytes,
        pool_size,
    })
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
    if has(PluginPermission::Network) && cfg.allow_network == Some(true) {
        tracing::warn!(
            "wasm plugin '{}' requests network access, which is not granted in this phase",
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
            wasm,
        }
    }

    #[test]
    fn limits_fall_back_to_defaults() {
        let limits = resolve_limits(&manifest_with(None, vec![]), 10000).expect("limits");
        assert_eq!(
            limits.memory_max_bytes,
            WASM_DEFAULT_MEMORY_MAX_MB * 1024 * 1024
        );
        assert_eq!(limits.fuel_limit, Some(WASM_DEFAULT_FUEL_LIMIT));
        assert_eq!(limits.call_timeout_ms, Some(10000));
        assert_eq!(limits.max_module_bytes, WASM_DEFAULT_MAX_MODULE_BYTES);
    }

    #[test]
    fn limits_reject_zero_memory_and_size() {
        let m = manifest_with(
            Some(WasmConfig {
                memory_max_mb: Some(0),
                ..Default::default()
            }),
            vec![],
        );
        assert!(resolve_limits(&m, 0).is_err());
    }

    #[test]
    fn pool_size_defaults_to_disabled_and_rejects_overflow() {
        let m = manifest_with(None, vec![]);
        assert_eq!(resolve_limits(&m, 100).expect("limits").pool_size, 0);

        let m = manifest_with(
            Some(WasmConfig {
                store_pool_size: Some(0),
                ..Default::default()
            }),
            vec![],
        );
        assert_eq!(resolve_limits(&m, 100).expect("limits").pool_size, 0);

        let m = manifest_with(
            Some(WasmConfig {
                store_pool_size: Some(4),
                ..Default::default()
            }),
            vec![],
        );
        assert_eq!(resolve_limits(&m, 100).expect("limits").pool_size, 4);

        let m = manifest_with(
            Some(WasmConfig {
                store_pool_size: Some(MAX_POOL_SIZE as u32 + 1),
                ..Default::default()
            }),
            vec![],
        );
        assert!(resolve_limits(&m, 100).is_err());
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
        let limits = resolve_limits(&m, 10000).expect("limits");
        assert_eq!(limits.fuel_limit, None);
        assert_eq!(limits.call_timeout_ms, Some(500));
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
}
