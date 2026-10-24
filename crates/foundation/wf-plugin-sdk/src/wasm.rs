//! Wasm plugin guest contract v1 (author- and host-visible).
//!
//! The contract has two guest surfaces with identical contribution
//! semantics: the core-module `wf_*` export contract below (linear-memory
//! string passing) and the component-model `wf:plugin/plugin` WIT world
//! (`crates/infra/wf-plugin/wit/plugin.wit`, WASI p2 context). JSON is the
//! only data encoding on both paths, so guests in any language can
//! implement the contract without shared bindings.
//!
//! Middleware guests answer a dispatch with either a JSON boolean:
//! continue or stop, context unchanged) or an envelope object
//! `{"proceed": bool, "context": <replacement>}` (both keys optional,
//! defaulting to `true` and the incoming context). The host threads the
//! returned context through the rest of the chain.

/// Contract version implemented by the host loader. Guests should treat a
/// mismatch as a load failure.
pub const WF_WASM_ABI_VERSION_V1: u32 = 1;

/// WIT world implemented by the component-model host
/// (`wasm/component.rs` against `wit/plugin.wit`). Compatibility compares
/// only the major version; the full string stays pinned while minor
/// differences use feature detection.
pub const WF_WASM_WORLD: &str = "wf:plugin@1.0.0";

/// Host ABI versions supported by this host, highest first. Negotiation
/// selects the highest version present in both the guest declaration and
/// this list.
pub const WF_WASM_SUPPORTED_ABI_VERSIONS: [u32; 1] = [WF_WASM_ABI_VERSION_V1];

/// Guest export names. `MEMORY` and `ALLOC` are required; `DEALLOC` is
/// optional (without it, per-call allocations are dropped with the store).
/// `HEAP_RESET` is optional: guests exporting it opt into store reuse, and
/// the host calls it after each pooled use to restore the allocator.
/// Lifecycle hooks are optional and default to success when absent.
/// `REGISTER` is optional (no contributions when absent); `DISPATCH` is
/// required only when the registration declares contributions.
/// `SUPPORTED_ABI_VERSIONS` is required: guests export
/// `wf_supported_abi_versions() -> u64` returning a packed `(ptr, len)`
/// to a `u32` array of supported ABI major versions; the host selects the
/// highest mutually supported version. `LAST_ERROR` is optional: guests may export
/// `wf_last_error() -> i64` (same packed `(ptr, len)` as `REGISTER`)
/// carrying a short UTF-8 detail string for the last failed hook call.
pub mod export {
    pub const MEMORY: &str = "memory";
    pub const ALLOC: &str = "alloc";
    pub const DEALLOC: &str = "dealloc";
    pub const HEAP_RESET: &str = "wf_heap_reset";
    pub const SUPPORTED_ABI_VERSIONS: &str = "wf_supported_abi_versions";
    pub const LAST_ERROR: &str = "wf_last_error";
    pub const ON_LOAD: &str = "wf_on_load";
    pub const ON_ACTIVATE: &str = "wf_on_activate";
    pub const ON_DEACTIVATE: &str = "wf_on_deactivate";
    pub const ON_UNLOAD: &str = "wf_on_unload";
    pub const ON_CONFIG_CHANGE: &str = "wf_on_config_change";
    pub const REGISTER: &str = "wf_register";
    pub const DISPATCH: &str = "wf_dispatch";
}

/// Host-provided imports a wasm guest may optionally use.
///
/// These imports are always available; guests that never import them are
/// unaffected. Importing them on an old host fails loudly at instantiation.
pub mod host {
    /// Module name of the core-module host import namespace.
    pub const MODULE: &str = "wf_host";
    /// Structured log import: `(level: u32, ptr: u32, len: u32)` pointing at
    /// a UTF-8 message in guest memory. Best-effort on the host side: unreadable
    /// input is dropped, never traps.
    pub const LOG: &str = "log";
    /// Log levels for the `log` import. Values at or above `LEVEL_ERROR`
    /// are recorded as errors.
    pub const LEVEL_TRACE: u32 = 0;
    pub const LEVEL_DEBUG: u32 = 1;
    pub const LEVEL_INFO: u32 = 2;
    pub const LEVEL_WARN: u32 = 3;
    pub const LEVEL_ERROR: u32 = 4;
    /// Single log messages longer than this are truncated by the host.
    pub const LOG_MESSAGE_CAP_BYTES: usize = 4096;
}

/// Contribution declaration returned by the `wf_register` export.
///
/// The guest returns a JSON object of this shape as packed `(ptr, len)`
/// pointing into its linear memory. Unknown fields are ignored so newer
/// hosts stay compatible with older guests.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct WasmContributionDecl {
    #[serde(default)]
    pub node_types: Vec<String>,
    #[serde(default)]
    pub tool_types: Vec<String>,
    #[serde(default)]
    pub llm_providers: Vec<String>,
    /// Contributed provider authentication implementations, dispatched on
    /// the `llm-auth` handler type.
    #[serde(default)]
    pub llm_auths: Vec<String>,
    #[serde(default)]
    pub event_handlers: Vec<String>,
    #[serde(default)]
    pub middleware: Vec<WasmMiddlewareDecl>,
    /// Declarative tool definitions contributed by the guest. Each entry is
    /// a `wf_types::tool::Tool` JSON document; unknown fields are ignored so
    /// older hosts stay compatible. Requires the `tool` permission.
    #[serde(default)]
    pub tools: Vec<serde_json::Value>,
    /// Declarative tool descriptions (`wf_types::tool_description::ToolDescriptionData`).
    #[serde(default)]
    pub tool_descriptions: Vec<serde_json::Value>,
}

/// One middleware registration declared by a wasm guest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct WasmMiddlewareDecl {
    pub phase: String,
    #[serde(default)]
    pub priority: i32,
}

/// Input envelope passed to lifecycle hook exports.
///
/// Serialized to JSON, written into guest memory via `alloc`, and passed
/// as `(ptr, len)`. The hook returns `0` on success.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WasmHookInput {
    pub plugin_id: String,
    pub config: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decl_ignores_unknown_fields() {
        let raw = r#"{"tool_types":["t"],"future_kind":["x"]}"#;
        let decl: WasmContributionDecl = serde_json::from_str(raw).expect("parse decl");
        assert_eq!(decl.tool_types, vec!["t".to_owned()]);
        assert!(decl.node_types.is_empty());
    }

    #[test]
    fn hook_input_round_trip() {
        let input = WasmHookInput {
            plugin_id: "demo".into(),
            config: serde_json::json!({"a": 1}),
        };
        let text = serde_json::to_string(&input).expect("serialize");
        assert!(text.contains("demo"));
        let back: WasmHookInput = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back.plugin_id, "demo");
    }
}
