use super::*;

pub(crate) fn component_call_error(
    inner: &ComponentPluginInner,
    op: &str,
    err: anyhow::Error,
) -> PluginError {
    let id = &inner.manifest.id;
    if let Some(trap) = err.downcast_ref::<wasmtime::Trap>() {
        if matches!(trap, wasmtime::Trap::Interrupt) {
            return PluginError::Timeout {
                plugin_id: id.clone(),
            };
        }
        if matches!(trap, wasmtime::Trap::OutOfFuel) {
            return PluginError::WasmError(format!(
                "plugin '{id}' component call '{op}' exhausted its fuel budget"
            ));
        }
    }
    super::super::pool::wasm_err(&format!("plugin '{id}' component call '{op}' failed"), err)
}

/// Drain captured guest `stdout`/`stderr` to the host log. Runs after every
/// guest call (success or failure) so failures stay diagnosable.
pub(crate) fn drain_component_stdio(
    inner: &ComponentPluginInner,
    op: &str,
    store: &Store<ComponentHostState>,
) {
    let state = store.data();
    super::super::stdio::drain_guest_stdio(&inner.manifest.id, op, &state.stdout, &state.stderr);
}

/// Credit fuel consumed by one component call. The timeout guard records
/// the call outcome without fuel (the store is mutably borrowed by the
/// in-flight call), so the caller snapshots fuel before the call and
/// settles the delta here, mirroring the core-module `observe` path.
pub(crate) fn record_component_fuel(
    inner: &ComponentPluginInner,
    store: &Store<ComponentHostState>,
    fuel_before: Option<u64>,
) {
    if let Some(before) = fuel_before {
        if let Ok(remaining) = store.get_fuel() {
            inner
                .stats
                .record_fuel_used(before.saturating_sub(remaining));
        }
    }
}

/// Snapshot metered fuel before a component call; `None` when unmetered.
pub(crate) fn component_fuel_before(
    inner: &ComponentPluginInner,
    store: &Store<ComponentHostState>,
) -> Option<u64> {
    inner.limits.fuel_limit.and_then(|_| store.get_fuel().ok())
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HookOp {
    Load,
    Activate,
    Deactivate,
    Unload,
    ConfigChange,
}

impl HookOp {
    pub fn name(self) -> &'static str {
        match self {
            HookOp::Load => "on-load",
            HookOp::Activate => "on-activate",
            HookOp::Deactivate => "on-deactivate",
            HookOp::Unload => "on-unload",
            HookOp::ConfigChange => "on-config-change",
        }
    }
}
