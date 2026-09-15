use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Lua,
    Native,
    Wasm,
}

/// Declared capability a plugin may touch. Declaration-only in this phase:
/// the engine treats permissions as audit/policy input (blocklist matching),
/// not as runtime capability enforcement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermission {
    Filesystem,
    Network,
    Shell,
    Environment,
    Llm,
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub plugin_type: Option<PluginType>,
    pub sdk_version: Option<String>,
    pub entry_point: String,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub optional_dependencies: HashMap<String, String>,
    #[serde(default)]
    pub contributions: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    /// Optional JSON Schema (Draft 2020-12 subset: `type: object`, `required`,
    /// `properties.<k>.type`) describing the plugin's `config` payload. When
    /// present the host validates configs against it; absent means free-form.
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub hooks: Option<HashMap<String, String>>,
    /// Wasm-only execution limits and WASI grants. Ignored for other types.
    #[serde(default)]
    pub wasm: Option<WasmConfig>,
}

/// Execution limits and WASI capability grants for `Wasm` plugins.
///
/// Every field is optional; the host falls back to conservative defaults
/// when a field is absent. Capabilities not granted here stay disabled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WasmConfig {
    /// Linear-memory cap in MiB. Defaults to `WASM_DEFAULT_MEMORY_MAX_MB`.
    #[serde(default)]
    pub memory_max_mb: Option<u64>,
    /// Fuel budget per guest call. Defaults to `WASM_DEFAULT_FUEL_LIMIT`.
    /// `0` disables metering.
    #[serde(default)]
    pub fuel_limit: Option<u64>,
    /// Per-call wall-clock timeout in ms enforced via epoch interruption.
    /// `None` falls back to the engine guard timeout.
    #[serde(default)]
    pub call_timeout_ms: Option<u64>,
    /// Maximum accepted `.wasm` module size in bytes.
    /// Defaults to `WASM_DEFAULT_MAX_MODULE_BYTES`.
    #[serde(default)]
    pub max_module_bytes: Option<u64>,
    /// Host directories preopened for the guest. Requires the `filesystem`
    /// permission; empty means no filesystem access.
    #[serde(default)]
    pub allowed_dirs: Option<Vec<String>>,
    /// Environment variable name prefixes visible to the guest. Requires
    /// the `environment` permission; empty means a minimal environment.
    #[serde(default)]
    pub allowed_env_prefixes: Option<Vec<String>>,
    /// Whether guest network access is allowed. Always denied in this
    /// phase: setting it to `true` fails plugin loading with a clear
    /// error, so manifests must leave it absent or `false`. Even with the
    /// `network` permission present the host grants no socket access.
    #[serde(default)]
    pub allow_network: Option<bool>,
    /// Number of idle guest sessions retained for reuse. `None` or `0`
    /// disables pooling: every call builds a fresh store. Pooling only
    /// takes effect when the guest exports `wf_heap_reset`.
    #[serde(default)]
    pub store_pool_size: Option<u32>,
}

/// Default linear-memory cap (MiB) for wasm plugins.
pub const WASM_DEFAULT_MEMORY_MAX_MB: u64 = 64;
/// Default fuel budget per wasm guest call. `0` disables metering.
pub const WASM_DEFAULT_FUEL_LIMIT: u64 = 10_000_000;
/// Default maximum accepted wasm module size in bytes (32 MiB).
pub const WASM_DEFAULT_MAX_MODULE_BYTES: u64 = 32 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_permissions_snake_case() {
        let raw = r#"
id = "demo"
version = "1.0.0"
entry_point = "main.lua"
permissions = ["filesystem", "llm"]
"#;
        let manifest: PluginManifest = toml::from_str(raw).expect("parse manifest");
        assert_eq!(
            manifest.permissions,
            vec![PluginPermission::Filesystem, PluginPermission::Llm]
        );
    }

    #[test]
    fn manifest_rejects_unknown_permission() {
        let raw = r#"
id = "demo"
version = "1.0.0"
entry_point = "main.lua"
permissions = ["root_access"]
"#;
        assert!(toml::from_str::<PluginManifest>(raw).is_err());
    }

    #[test]
    fn manifest_parses_wasm_type_and_config() {
        let raw = r#"
id = "wasm-demo"
version = "1.0.0"
entry_point = "plugin.wasm"
plugin_type = "wasm"
permissions = ["filesystem"]

[wasm]
memory_max_mb = 32
fuel_limit = 1000000
allow_network = false
allowed_dirs = ["./data"]
store_pool_size = 4
"#;
        let manifest: PluginManifest = toml::from_str(raw).expect("parse manifest");
        assert_eq!(manifest.plugin_type, Some(PluginType::Wasm));
        let wasm = manifest.wasm.expect("wasm config present");
        assert_eq!(wasm.memory_max_mb, Some(32));
        assert_eq!(wasm.fuel_limit, Some(1_000_000));
        assert_eq!(wasm.allow_network, Some(false));
        assert_eq!(wasm.allowed_dirs, Some(vec!["./data".to_owned()]));
        assert_eq!(wasm.store_pool_size, Some(4));
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let raw = r#"
id = "demo"
version = "1.0.0"
entry_point = "main.lua"
bogus = true
"#;
        assert!(toml::from_str::<PluginManifest>(raw).is_err());
    }
}
