use super::GeneratedPlugin;
use super::*;

/// One live component session: its private store plus the bindings bound
/// to the instance inside it. Sessions are checked out exclusively, so
/// linear memory is never shared between concurrent calls.
pub struct ComponentSession {
    pub store: Store<ComponentHostState>,
    /// Generated bindings borrow nothing but their type lives in the
    /// private `gen` module, so the field stays crate-visible.
    pub(crate) bindings: GeneratedPlugin,
}

/// Bounded pool of idle component sessions for one plugin.
///
/// Mirrors the core-module `SessionPool`: acquisition never blocks (a miss
/// builds a fresh session), and only sessions from successful calls are
/// retained. Unlike the core path there is no heap-reset gate: the
/// canonical ABI manages guest memory through `cabi_realloc`, so reuse does
/// not corrupt a bump allocator. Guests must still tolerate instance reuse
/// (globals persist across pooled calls); pooling stays opt-in via
/// `store_pool_size` and off by default, so unpooled behavior is unchanged.
pub struct ComponentSessionPool {
    plugin_id: String,
    engine: Engine,
    pre: InstancePre<ComponentHostState>,
    grants: WasiGrants,
    limits: WasmLimits,
    idle: Option<std::sync::Mutex<std::collections::VecDeque<ComponentSession>>>,
    stats: Arc<WasmStats>,
}

impl ComponentSessionPool {
    /// Create the pool. The idle queue exists only when the manifest
    /// requests pooling (`pool_size > 0`); otherwise every call builds a
    /// fresh session exactly as before.
    pub fn new(
        plugin_id: &str,
        engine: &Engine,
        pre: &InstancePre<ComponentHostState>,
        grants: &WasiGrants,
        limits: &WasmLimits,
        stats: &Arc<WasmStats>,
    ) -> Self {
        let idle = (limits.pool_size > 0).then(|| {
            tracing::info!(
                "wasm component '{plugin_id}' session pool enabled (cap {})",
                limits.pool_size
            );
            std::sync::Mutex::new(std::collections::VecDeque::new())
        });
        Self {
            plugin_id: plugin_id.to_owned(),
            engine: engine.clone(),
            pre: pre.clone(),
            grants: grants.clone(),
            limits: limits.clone(),
            idle,
            stats: stats.clone(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.idle.is_some()
    }

    /// Check out a session: reuse an idle one when available (fuel is
    /// replenished first so reuse never starves on leftovers), otherwise
    /// build a fresh session. Never blocks.
    pub async fn acquire(&self) -> PluginResult<ComponentSession> {
        if let Some(idle) = &self.idle {
            let reused = idle
                .lock()
                .expect("component session pool poisoned")
                .pop_front();
            if let Some(mut session) = reused {
                self.stats.record_pool_hit();
                session
                    .store
                    .set_fuel(self.limits.fuel_limit.unwrap_or(u64::MAX))
                    .map_err(|e| super::super::pool::wasm_err("component fuel reset failed", e))?;
                return Ok(session);
            }
            self.stats.record_pool_miss();
        }
        new_component_session(
            &self.engine,
            &self.pre,
            &self.plugin_id,
            &self.grants,
            &self.limits,
        )
        .await
    }

    /// Return a session after use. Sessions from failed calls are always
    /// discarded because post-trap state is untrusted.
    pub async fn release(&self, session: ComponentSession, healthy: bool) {
        let Some(idle) = &self.idle else {
            return;
        };
        if !healthy {
            self.stats.record_pool_drop();
            return;
        }
        let mut idle = idle.lock().expect("component session pool poisoned");
        if idle.len() >= self.limits.pool_size {
            self.stats.record_pool_drop();
            return;
        }
        idle.push_back(session);
    }
}

/// Build one fresh component session: new store plus instantiation from
/// the shared pre-resolved imports plus host bindings.
async fn new_component_session(
    engine: &Engine,
    pre: &InstancePre<ComponentHostState>,
    plugin_id: &str,
    grants: &WasiGrants,
    limits: &WasmLimits,
) -> PluginResult<ComponentSession> {
    let mut store = build_component_store(engine, plugin_id, grants, limits)?;
    let instance = match pre.instantiate_async(&mut store).await {
        Ok(instance) => instance,
        Err(e) => {
            // Instantiation runs the guest's `_initialize`, so a trap here can
            // carry a guest-side reason on the captured stdio. Drain it before
            // returning so the failure is diagnosable.
            let (stdout, stderr) = super::super::stdio::drain_guest_stdio(
                plugin_id,
                "instantiate",
                &store.data().stdout,
                &store.data().stderr,
            );
            let guest = format!(
                "stdout=[{}] stderr=[{}]",
                String::from_utf8_lossy(&stdout).trim(),
                String::from_utf8_lossy(&stderr).trim()
            );
            return Err(super::super::pool::wasm_err(
                &format!("plugin '{plugin_id}' instantiate failed; guest {guest}"),
                e,
            ));
        }
    };
    let bindings = GeneratedPlugin::new(&mut store, &instance).map_err(|e| {
        super::super::pool::wasm_err(&format!("plugin '{plugin_id}' bind failed"), e)
    })?;
    // `instance` is a copyable handle owned by the store; dropping it here
    // destroys nothing, and later calls only need the store plus bindings.
    Ok(ComponentSession { store, bindings })
}
