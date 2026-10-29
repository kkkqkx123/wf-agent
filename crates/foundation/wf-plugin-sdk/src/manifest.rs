use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Lua,
    Native,
    Wasm,
}

impl PluginType {
    /// The crate feature that gates this backend. Single source of truth for
    /// the mapping used by `BackendNotCompiled` and any tooling that needs to
    /// name the feature for a backend.
    pub const fn feature_name(self) -> &'static str {
        match self {
            PluginType::Lua => "lua",
            PluginType::Native => "native",
            PluginType::Wasm => "wasm",
        }
    }
}
/// Declared capability a plugin may touch. Enforcement differs by backend:
/// wasm guests get runtime capability enforcement (WASI grants), while lua
/// and native declarations are admission and audit input only (blocklist
/// matching plus sandbox library removal for lua, nothing at runtime for
/// native). Never read a permission as a cross-backend security boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermission {
    Filesystem,
    Network,
    Shell,
    Environment,
    Llm,
    /// Register a low-level LLM wire-format codec (`llm-provider`
    /// contribution). Subdivides the coarse `llm` permission so a plugin
    /// that only contributes prompts cannot register formats.
    LlmCodec,
    /// Contribute a provider authentication implementation (`llm-auth`
    /// contribution: login / refresh / resolve). Implies network access to
    /// the provider's OAuth endpoints; credentials are persisted by the
    /// host's credential store, never by the plugin itself.
    LlmAuth,
    /// Contribute an image generation implementation (`llm-images`
    /// contribution: `generate_images`). Like `llm-auth`, implementations
    /// that hit the network must run on the native backend; Lua/Wasm
    /// sandboxes have no HTTP and can only serve deterministic test doubles.
    LlmImages,
    /// Perform LLM model discovery over the network for a contributed
    /// provider (default models endpoint or custom endpoint). Subdivides
    /// `network` so discovery can be granted without general socket access.
    LlmDiscovery,
    /// Contribute declarative tool definitions (`tool` contribution).
    Tool,
    /// Contribute tool descriptions (`tool-description` contribution).
    ToolDescription,
    /// Implement the codec through the native dispatch channel (structured
    /// codec round-trip). Subdivides the native backend surface.
    NativeCodec,
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
    pub entry_point: String,
    /// Version-keyed entry points for multi-version coexistence. Key is the
    /// ABI major version; value is the entry-point path. When the host
    /// negotiates a specific ABI version, it prefers the matching entry
    /// from this map and falls back to `entry_point` when absent.
    #[serde(default)]
    pub versioned_entry_points: HashMap<u32, String>,
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
    /// Declarative connection templates contributed by the plugin. Each
    /// entry is written into the host provider registry on activation
    /// (linked with the `llm-provider` codec contribution) and removed
    /// symmetrically on deactivation. The shape mirrors
    /// `wf_types::llm::LlmProviderDefinition`; maps stay generic JSON so
    /// this crate keeps no dependency on `wf-types`.
    #[serde(default)]
    pub llm_providers: Vec<PluginLlmProviderDefinition>,
    /// Declarative tool definitions (`wf_types::tool::Tool` JSON). Synced
    /// into the tool registry on activation; paired with `tool_types`
    /// executors from the same plugin via the bridge convention.
    #[serde(default)]
    pub tools: Vec<serde_json::Value>,
    /// Declarative tool descriptions
    /// (`wf_types::tool_description::ToolDescriptionData` JSON).
    #[serde(default)]
    pub tool_descriptions: Vec<serde_json::Value>,
    /// Wasm-only execution limits and WASI grants. Ignored for other types.
    #[serde(default)]
    pub wasm: Option<WasmConfig>,
    /// Lua-only execution limits. Ignored for other types.
    #[serde(default)]
    pub lua: Option<LuaConfig>,
}

/// Declarative LLM connection template in the plugin manifest.
///
/// Mirrors `wf_types::llm::LlmProviderDefinition` without depending on
/// `wf-types`: the host converts each entry at sync time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PluginLlmProviderDefinition {
    pub id: String,
    pub format: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub default_headers: Option<HashMap<String, Value>>,
    #[serde(default)]
    pub api_version: Option<String>,
    #[serde(default)]
    pub model_discovery: Option<PluginModelDiscovery>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
    /// Authentication declaration binding this provider to a registered
    /// auth implementation (built-in id or the plugin's own `llm-auth`
    /// contribution). Absent means static key authentication.
    #[serde(default)]
    pub auth: Option<PluginProviderAuthRef>,
}

/// Manifest-side reference from a provider definition to a registered auth
/// implementation. Mirrors `wf_types::llm::ProviderAuthRef`; maps stay
/// generic JSON so this crate keeps no dependency on `wf-types`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginProviderAuthRef {
    /// Name of the registered auth implementation (built-in id or plugin
    /// contribution name).
    pub provider: String,
    /// Authentication method the declaration expects (`api_key` / `oauth`).
    #[serde(default)]
    pub method: Option<String>,
    /// True when access through this provider is subscription-backed.
    #[serde(default)]
    pub is_subscription: Option<bool>,
    /// Provider-specific auth parameters (endpoints, client id, scopes,
    /// flow). Never carries secrets; passed to the named implementation
    /// verbatim at login time.
    #[serde(default)]
    pub params: Option<HashMap<String, Value>>,
}

/// Model discovery description in the plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginModelDiscovery {
    ModelsEndpoint {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        json_path: Option<String>,
    },
    CustomEndpoint {
        url: String,
        #[serde(default)]
        method: Option<String>,
        #[serde(default)]
        headers: Option<HashMap<String, Value>>,
        #[serde(default)]
        json_path: Option<String>,
    },
    StaticList {
        models: Vec<PluginModelInfo>,
    },
    Disabled,
}

/// Model entry in a manifest static list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PluginModelInfo {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_window_size: Option<u32>,
    #[serde(default)]
    pub metadata: Option<HashMap<String, Value>>,
}

/// Execution limits and WASI capability grants for `Wasm` plugins.
///
/// Every field is optional; the host falls back to engine-global defaults
/// and then to conservative built-ins when a field is absent. Limits never
/// fail loading: `call_timeout_ms: Some(0)` disables the epoch deadline,
/// `memory_max_mb` and `max_module_bytes` of `Some(0)` fall back to their
/// built-ins, and an over-large `store_pool_size` is clamped.
/// Capabilities not granted here stay disabled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WasmConfig {
    /// Linear-memory cap in MiB. Defaults to `WASM_DEFAULT_MEMORY_MAX_MB`;
    /// `Some(0)` falls back to that default.
    #[serde(default)]
    pub memory_max_mb: Option<u64>,
    /// Fuel budget per guest call. Defaults to `WASM_DEFAULT_FUEL_LIMIT`.
    /// `0` disables metering.
    #[serde(default)]
    pub fuel_limit: Option<u64>,
    /// Per-call wall-clock timeout in ms enforced via epoch interruption.
    /// `None` falls back to the engine guard timeout; `Some(0)` disables
    /// the epoch deadline.
    #[serde(default)]
    pub call_timeout_ms: Option<u64>,
    /// Maximum accepted `.wasm` module size in bytes.
    /// Defaults to `WASM_DEFAULT_MAX_MODULE_BYTES`; `Some(0)` falls back
    /// to that default.
    #[serde(default)]
    pub max_module_bytes: Option<u64>,
    /// Host directories preopened for the guest. Requires the `filesystem`
    /// permission; empty means no filesystem access.
    #[serde(default)]
    pub allowed_dirs: Option<Vec<String>>,
    /// Host directories preopened with write access. Requires the
    /// `filesystem` permission; a directory listed here is readable and
    /// writable even when it also appears in `allowed_dirs`. Empty means
    /// the guest cannot write to the host filesystem.
    #[serde(default)]
    pub allowed_write_dirs: Option<Vec<String>>,
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

/// Execution limits for `Lua` plugins.
///
/// Every field is optional; the host falls back to engine-global defaults
/// and then to conservative built-ins when a field is absent. Limits never
/// fail loading: `timeout_ms: Some(0)` disables the hook deadline (the
/// outer guard still applies), and `memory_limit_kb: Some(0)` falls back
/// to the built-in default because memory has no outer backstop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LuaConfig {
    /// Per-call wall-clock timeout in ms enforced via the interpreter hook.
    /// Defaults to `LUA_DEFAULT_TIMEOUT_MS`; `Some(0)` disables the hook
    /// deadline.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Interpreter memory cap in KiB. Defaults to
    /// `LUA_DEFAULT_MEMORY_LIMIT_KB`; `Some(0)` falls back to that default.
    #[serde(default)]
    pub memory_limit_kb: Option<usize>,
}

/// Default per-call timeout (ms) for lua plugins.
pub const LUA_DEFAULT_TIMEOUT_MS: u64 = 5_000;
/// Default interpreter memory cap (KiB) for lua plugins.
pub const LUA_DEFAULT_MEMORY_LIMIT_KB: usize = 64 * 1024;

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
    fn manifest_parses_wasm_write_dirs() {
        let raw = r#"
id = "wasm-write"
version = "1.0.0"
entry_point = "plugin.wasm"
plugin_type = "wasm"
permissions = ["filesystem"]

[wasm]
allowed_dirs = ["./data"]
allowed_write_dirs = ["./cache"]
"#;
        let manifest: PluginManifest = toml::from_str(raw).expect("parse manifest");
        let wasm = manifest.wasm.expect("wasm config present");
        assert_eq!(wasm.allowed_dirs, Some(vec!["./data".to_owned()]));
        assert_eq!(wasm.allowed_write_dirs, Some(vec!["./cache".to_owned()]));
    }

    #[test]
    fn manifest_parses_lua_limits() {
        let raw = r#"
id = "lua-demo"
version = "1.0.0"
entry_point = "main.lua"
plugin_type = "lua"

[lua]
timeout_ms = 2000
memory_limit_kb = 8192
"#;
        let manifest: PluginManifest = toml::from_str(raw).expect("parse manifest");
        let lua = manifest.lua.expect("lua config present");
        assert_eq!(lua.timeout_ms, Some(2000));
        assert_eq!(lua.memory_limit_kb, Some(8192));
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

    #[test]
    fn manifest_parses_llm_auth_permission_and_provider_auth_section() {
        let raw = r#"
id = "claude-sub"
version = "1.0.0"
entry_point = "main.lua"
permissions = ["llm_codec", "llm_auth", "llm_discovery"]

[[llm_providers]]
id = "claude-subscription"
format = "ANTHROPIC"
base_url = "https://api.anthropic.com"
auth_type = "bearer"

[llm_providers.auth]
provider = "claude-subscription"
method = "oauth"
is_subscription = true

[llm_providers.auth.params]
authorize_url = "https://claude.ai/oauth/authorize"
token_url = "https://console.anthropic.com/v1/oauth/token"
client_id = "cid"
flow = "pkce_paste"
"#;
        let manifest: PluginManifest = toml::from_str(raw).expect("parse manifest");
        assert!(manifest.permissions.contains(&PluginPermission::LlmAuth));
        let provider = manifest.llm_providers.first().expect("provider");
        let auth = provider.auth.as_ref().expect("auth section");
        assert_eq!(auth.provider, "claude-subscription");
        assert_eq!(auth.method.as_deref(), Some("oauth"));
        assert_eq!(auth.is_subscription, Some(true));
        let params = auth.params.as_ref().expect("params");
        assert_eq!(
            params.get("flow").and_then(Value::as_str),
            Some("pkce_paste")
        );
        assert_eq!(params.get("client_id").and_then(Value::as_str), Some("cid"));
    }
}
