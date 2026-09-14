//! Wasm plugin guest contract v1 (author- and host-visible).
//!
//! The contract mirrors the native C ABI v1 contribution model but uses
//! linear-memory string passing instead of C pointers: the guest exports a
//! `memory`, an `alloc` bump allocator, and a fixed set of `wf_*` functions.
//! JSON is the only data encoding, so guests in any language can implement
//! the contract without shared bindings.
//!
//! Long-term direction is a `wf:plugin/plugin` WIT world; until then this
//! core-module contract is the stable surface.

/// Contract version implemented by the host loader. Guests should treat a
/// mismatch as a load failure.
pub const WF_WASM_ABI_VERSION: u32 = 1;

/// Reserved WIT world name for the future component-model contract.
pub const WF_WASM_WORLD: &str = "wf:plugin/plugin";

/// Guest export names. `MEMORY` and `ALLOC` are required; `DEALLOC` is
/// optional (without it, per-call allocations are dropped with the store).
/// `HEAP_RESET` is optional: guests exporting it opt into store reuse, and
/// the host calls it after each pooled use to restore the allocator.
/// Lifecycle hooks are optional and default to success when absent.
/// `REGISTER` is optional (no contributions when absent); `DISPATCH` is
/// required only when the registration declares contributions.
pub mod export {
    pub const MEMORY: &str = "memory";
    pub const ALLOC: &str = "alloc";
    pub const DEALLOC: &str = "dealloc";
    pub const HEAP_RESET: &str = "wf_heap_reset";
    pub const ON_LOAD: &str = "wf_on_load";
    pub const ON_ACTIVATE: &str = "wf_on_activate";
    pub const ON_DEACTIVATE: &str = "wf_on_deactivate";
    pub const ON_UNLOAD: &str = "wf_on_unload";
    pub const ON_CONFIG_CHANGE: &str = "wf_on_config_change";
    pub const REGISTER: &str = "wf_register";
    pub const DISPATCH: &str = "wf_dispatch";
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
    #[serde(default)]
    pub formatters: Vec<String>,
    #[serde(default)]
    pub event_handlers: Vec<String>,
    #[serde(default)]
    pub middleware: Vec<WasmMiddlewareDecl>,
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
