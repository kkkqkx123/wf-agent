pub mod abi;
pub mod component;
pub mod loader;
pub mod plugin;
pub mod policy;
pub mod pool;
pub mod shared;
pub mod stats;
pub mod stdio;

pub use loader::{
    load_wasm_plugin, load_wasm_plugin_verified_with_base, load_wasm_plugin_with_base,
};
pub use plugin::WasmPlugin;
pub use policy::{resolve_grants, resolve_limits, WasiGrants, WasmLimits};
pub use stats::{WasmStats, WasmStatsSnapshot};

use wf_plugin_sdk::wasm::WasmMiddlewareDecl;
use wf_types::MiddlewarePhase;

/// Emit a warning for each middleware entry whose phase the engine does not
/// dispatch. Shared by the core-module and component loaders so the check
/// lives in one place (see each loader's `register_contributions`).
pub(crate) fn warn_unknown_middleware_phases(plugin_id: &str, middleware: &[WasmMiddlewareDecl]) {
    for mw in middleware {
        let phase = MiddlewarePhase::from(mw.phase.as_str());
        if !phase.is_known() {
            tracing::warn!(
                "wasm plugin '{}' registers middleware for unknown phase '{}'; it will never be dispatched",
                plugin_id, mw.phase
            );
        }
    }
}
