//! Component-model guest host (`wf:plugin/plugin` world).
//!
//! Dual-compat companion to the core-module path in `plugin.rs`: component
//! binaries (detected by magic in the loader) run through `bindgen!`
//! bindings with a WASI p2 context instead of the `wf_*` export contract.
//! The `Plugin` trait surface, contribution semantics, limits, and stats
//! stay identical, so callers never observe which guest kind they use.
//!
//! Calls use the async component API: the shared engine enables async
//! support, so both instantiation and calls go through `*_async` on the
//! calling task (mirroring the core-module path). Fuel metering and epoch
//! interruption apply unchanged, with the outer timeout as backstop.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use wasmtime::component::{Component, InstancePre, Linker, ResourceTable};
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView};
use wf_plugin_sdk::wasm::{WasmContributionDecl, WasmMiddlewareDecl};
use wf_types::MiddlewarePhase;

use super::policy::{validate_network_policy, WasiGrants, WasmLimits};
use super::pool::{self, wasm_err};
use super::stats::WasmStats;
use super::stdio::{GuestLogPipe, GUEST_STDIO_CAP_BYTES};
use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::contributions::types::*;
use crate::contributions::NextFn;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin as PluginTrait;

/// Generated component bindings kept in a child module so the `Plugin`
/// struct produced by `bindgen!` does not shadow the host trait.
mod gen {
    wasmtime::component::bindgen!({
        world: "plugin",
        path: ["wit/plugin.wit"],
        async: true,
    });
}

use gen::exports::wf::plugin::lifecycle::HookInput;
/// Type alias for the generated world struct, avoiding name clashes.
use gen::Plugin as GeneratedPlugin;

/// WASI p2 view for component guests: context plus the resource table and
/// the memory limiter. Capabilities default to closed; grants are applied
/// per plugin at session build time. Public like the core-module
/// `WasmStoreState`: external callers see the type, construction stays
/// inside the pool.
pub struct ComponentHostState {
    ctx: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    /// Owning plugin id, used to attribute guest-to-host log records.
    plugin_id: String,
    /// Guest `stdout` capture, drained to the host log after every call.
    pub(crate) stdout: GuestLogPipe,
    /// Guest `stderr` capture, drained like `stdout`.
    pub(crate) stderr: GuestLogPipe,
}

impl IoView for ComponentHostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for ComponentHostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

/// Pinned component-world version. The WIT package
/// (`wit/plugin.wit`) and these bindings must agree; a world change
/// requires a version bump on both sides, and old guests then fail
/// loudly at bind time instead of silently misbehaving.
pub(crate) const WF_COMPONENT_WORLD_VERSION: &str = "0.1.0-draft";

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
                    .map_err(|e| wasm_err("component fuel reset failed", e))?;
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
    let instance = pre
        .instantiate_async(&mut store)
        .await
        .map_err(|e| wasm_err(&format!("plugin '{plugin_id}' instantiate failed"), e))?;
    let bindings = GeneratedPlugin::new(&mut store, &instance)
        .map_err(|e| wasm_err(&format!("plugin '{plugin_id}' bind failed"), e))?;
    // `instance` is a copyable handle owned by the store; dropping it here
    // destroys nothing, and later calls only need the store plus bindings.
    Ok(ComponentSession { store, bindings })
}

pub(crate) struct ComponentPluginInner {
    pub(crate) manifest: PluginManifest,
    pub(crate) engine: Engine,
    pub(crate) limits: WasmLimits,
    /// Cached contribution declaration. Locked because
    /// `reload_declaration` may replace it while registered handlers hold
    /// the same inner.
    pub(crate) decl: std::sync::RwLock<WasmContributionDecl>,
    pub(crate) stats: Arc<WasmStats>,
    /// Idle session pool. Disabled by default; enabled only when the
    /// manifest requests it. The pool owns the pre-resolved imports and
    /// WASI grants, so every session it builds carries the same
    /// configuration.
    pub(crate) pool: ComponentSessionPool,
}

pub struct ComponentPlugin {
    inner: Arc<ComponentPluginInner>,
}

impl ComponentPlugin {
    pub(crate) fn from_inner(inner: ComponentPluginInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn stats(&self) -> super::stats::WasmStatsSnapshot {
        self.inner.stats.snapshot()
    }
}

/// Host implementation of the `wf:plugin/host-log` world import: structured
/// guest-to-host logging. Best-effort; over-long messages are truncated by
/// the shared emitter and the call never fails the guest.
impl gen::wf::plugin::host_log::Host for ComponentHostState {
    async fn log(&mut self, level: u32, message: String) {
        let plugin_id = self.plugin_id.clone();
        super::stdio::emit_host_log(&plugin_id, level, message.as_bytes());
    }
}

/// Marker connecting the generated world bindings to [`ComponentHostState`].
struct ComponentHostView;

impl wasmtime::component::HasData for ComponentHostView {
    type Data<'a> = &'a mut ComponentHostState;
}

/// Build the linker shared by every component plugin: WASI p2 imports plus
/// the `wf:plugin/host-log` guest-to-host namespace.
fn new_component_linker(engine: &Engine) -> PluginResult<Linker<ComponentHostState>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| wasm_err("component wasi linker setup failed", e))?;
    GeneratedPlugin::add_to_linker::<ComponentHostState, ComponentHostView>(&mut linker, |state| {
        state
    })
    .map_err(|e| wasm_err("component host-log linker setup failed", e))?;
    Ok(linker)
}

/// Build one component session: WASI context from grants, fuel budget and
/// memory cap from limits. Runs on the caller's thread; component entry
/// points invoke it inside `spawn_blocking`.
fn build_component_store(
    engine: &Engine,
    plugin_id: &str,
    grants: &WasiGrants,
    limits: &WasmLimits,
) -> PluginResult<Store<ComponentHostState>> {
    let stdout = GuestLogPipe::new(GUEST_STDIO_CAP_BYTES);
    let stderr = GuestLogPipe::new(GUEST_STDIO_CAP_BYTES);
    let mut builder = WasiCtxBuilder::new();
    builder.stdout(stdout.clone()).stderr(stderr.clone());
    for (key, value) in &grants.env_vars {
        builder.env(key, value);
    }
    for (host_path, guest_path) in &grants.preopened_dirs {
        if !std::path::Path::new(host_path).is_dir() {
            tracing::warn!(
                "wasm component '{plugin_id}' preopen skipped, not a directory: {host_path}"
            );
            continue;
        }
        if let Err(e) = builder.preopened_dir(
            host_path,
            guest_path,
            wasmtime_wasi::DirPerms::READ,
            wasmtime_wasi::FilePerms::READ,
        ) {
            tracing::warn!("wasm component '{plugin_id}' preopen failed for '{host_path}': {e}");
        }
    }
    for (host_path, guest_path) in &grants.writable_dirs {
        if !std::path::Path::new(host_path).is_dir() {
            tracing::warn!(
                "wasm component '{plugin_id}' writable preopen skipped, not a directory: {host_path}"
            );
            continue;
        }
        if let Err(e) = builder.preopened_dir(
            host_path,
            guest_path,
            wasmtime_wasi::DirPerms::READ | wasmtime_wasi::DirPerms::MUTATE,
            wasmtime_wasi::FilePerms::READ | wasmtime_wasi::FilePerms::WRITE,
        ) {
            tracing::warn!(
                "wasm component '{plugin_id}' writable preopen failed for '{host_path}': {e}"
            );
        }
    }
    let mut store = Store::new(
        engine,
        ComponentHostState {
            ctx: builder.build(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_max_bytes as usize)
                .build(),
            plugin_id: plugin_id.to_owned(),
            stdout,
            stderr,
        },
    );
    store.limiter(|state: &mut ComponentHostState| {
        &mut state.limits as &mut dyn wasmtime::ResourceLimiter
    });
    store
        .set_fuel(limits.fuel_limit.unwrap_or(u64::MAX))
        .map_err(|e| wasm_err("component fuel setup failed", e))?;
    Ok(store)
}

fn component_call_error(inner: &ComponentPluginInner, op: &str, err: anyhow::Error) -> PluginError {
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
    wasm_err(&format!("plugin '{id}' component call '{op}' failed"), err)
}

/// Run one component operation with the outer timeout backstop. The epoch
/// deadline (armed on the store before the call) fires first for spinning
/// guests; this only guards host-side stalls. Every outcome is recorded in
/// stats with its real error value so timeouts and failures stay distinct.
async fn guard_component_call<T>(
    plugin_id: &str,
    limits: &WasmLimits,
    stats: &WasmStats,
    call: impl std::future::Future<Output = PluginResult<T>>,
) -> PluginResult<T> {
    let outcome = match pool::outer_timeout_ms(limits.call_timeout_ms) {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), call)
            .await
            .map_err(|_| PluginError::Timeout {
                plugin_id: plugin_id.to_owned(),
            })?,
        None => call.await,
    };
    match outcome {
        Ok(value) => {
            stats.record(&Ok(()), None);
            Ok(value)
        }
        Err(err) => {
            let probe: PluginResult<()> = match &err {
                PluginError::Timeout { plugin_id } => Err(PluginError::Timeout {
                    plugin_id: plugin_id.clone(),
                }),
                other => Err(PluginError::WasmError(other.to_string())),
            };
            stats.record(&probe, None);
            Err(err)
        }
    }
}

/// Drain captured guest `stdout`/`stderr` to the host log. Runs after every
/// guest call (success or failure) so failures stay diagnosable.
fn drain_component_stdio(
    inner: &ComponentPluginInner,
    op: &str,
    store: &Store<ComponentHostState>,
) {
    let state = store.data();
    super::stdio::drain_guest_stdio(&inner.manifest.id, op, &state.stdout, &state.stderr);
}

/// Credit fuel consumed by one component call. The timeout guard records
/// the call outcome without fuel (the store is mutably borrowed by the
/// in-flight call), so the caller snapshots fuel before the call and
/// settles the delta here, mirroring the core-module `observe` path.
fn record_component_fuel(
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
fn component_fuel_before(
    inner: &ComponentPluginInner,
    store: &Store<ComponentHostState>,
) -> Option<u64> {
    inner.limits.fuel_limit.and_then(|_| store.get_fuel().ok())
}

/// Call one lifecycle hook. The session is checked out from the pool and
/// returned afterwards; only sessions from successful calls are eligible
/// for reuse. A component instantiates only when it implements the full
/// world, so unlike the tolerant core-module path a bind failure here
/// means host/guest skew and fails loudly.
async fn invoke_component_hook(
    inner: &Arc<ComponentPluginInner>,
    op: HookOp,
    input: Option<HookInput>,
) -> PluginResult<()> {
    let name = op.name();
    let mut session = inner.pool.acquire().await?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let lifecycle = session.bindings.wf_plugin_lifecycle();
    let fuel_before = component_fuel_before(inner, &session.store);
    let call = async {
        let result = match (op, input) {
            (HookOp::Load, Some(input)) => lifecycle.call_on_load(&mut session.store, &input).await,
            (HookOp::Activate, Some(input)) => {
                lifecycle.call_on_activate(&mut session.store, &input).await
            }
            (HookOp::Deactivate, None) => lifecycle.call_on_deactivate(&mut session.store).await,
            (HookOp::Unload, None) => lifecycle.call_on_unload(&mut session.store).await,
            (HookOp::ConfigChange, Some(input)) => {
                lifecycle
                    .call_on_config_change(&mut session.store, &input.config)
                    .await
            }
            _ => {
                return Err(PluginError::Internal(format!(
                    "hook '{}' called with wrong input shape",
                    op.name()
                )));
            }
        }
        .map_err(|e| component_call_error(inner, name, e))?;
        result.map_err(|e| {
            PluginError::WasmError(format!(
                "plugin '{}' hook '{name}' failed: {e}",
                inner.manifest.id
            ))
        })
    };
    let outcome = guard_component_call(&inner.manifest.id, &inner.limits, &inner.stats, call).await;
    record_component_fuel(inner, &session.store, fuel_before);
    drain_component_stdio(inner, name, &session.store);
    inner.pool.release(session, outcome.is_ok()).await;
    outcome
}

#[derive(Debug, Clone, Copy)]
enum HookOp {
    Load,
    Activate,
    Deactivate,
    Unload,
    ConfigChange,
}

impl HookOp {
    fn name(self) -> &'static str {
        match self {
            HookOp::Load => "on-load",
            HookOp::Activate => "on-activate",
            HookOp::Deactivate => "on-deactivate",
            HookOp::Unload => "on-unload",
            HookOp::ConfigChange => "on-config-change",
        }
    }
}

/// Fetch the contribution declaration by calling `register` once at load.
/// The session is checked out from the pool and returned afterwards, so a
/// pooled plugin starts life with one warm idle session.
pub(crate) async fn fetch_component_declaration(
    inner: &ComponentPluginInner,
) -> PluginResult<WasmContributionDecl> {
    let limits = inner.limits.clone();
    let stats = inner.stats.clone();
    let plugin_id = inner.manifest.id.clone();
    let mut session = inner.pool.acquire().await?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let contributions = session.bindings.wf_plugin_contributions();
    let fuel_before = component_fuel_before(inner, &session.store);
    let decl = guard_component_call(&plugin_id, &limits, &stats, async {
        contributions
            .call_register(&mut session.store)
            .await
            .map_err(|e| wasm_err("register call failed", e))
    })
    .await;
    record_component_fuel(inner, &session.store, fuel_before);
    drain_component_stdio(inner, "register", &session.store);
    let healthy = decl.is_ok();
    let decl = decl?;
    let mapped = WasmContributionDecl {
        node_types: decl.node_types,
        tool_types: decl.tool_types,
        llm_providers: decl.llm_providers,
        event_handlers: decl.event_handlers,
        middleware: decl
            .middleware
            .into_iter()
            .map(|m| WasmMiddlewareDecl {
                phase: m.phase,
                priority: m.priority,
            })
            .collect(),
    };
    inner.pool.release(session, healthy).await;
    Ok(mapped)
}

/// Load a component-model plugin from already-read bytes.
/// Mirrors the core-module loader: path validation and the initial read
/// happen in `loader.rs`; this function compiles, pre-resolves, and fetches
/// the declaration without re-reading the artifact from disk.
pub(crate) async fn load_component_plugin_at(
    manifest: &PluginManifest,
    bytes: &[u8],
    limits: &WasmLimits,
    grants: &WasiGrants,
) -> PluginResult<Arc<dyn PluginTrait>> {
    let id = manifest.id.clone();
    validate_network_policy(manifest)?;
    tracing::debug!(
        "wasm component '{id}' loading against world version {WF_COMPONENT_WORLD_VERSION}"
    );
    if bytes.len() as u64 > limits.max_module_bytes {
        return Err(PluginError::LoadFailed(format!(
            "wasm component for plugin '{id}' ({} bytes) exceeds limit of {} bytes",
            bytes.len(),
            limits.max_module_bytes
        )));
    }
    let engine = pool::engine_handle();
    let component = cached_component(&engine, bytes).map_err(|e| match e {
        PluginError::WasmError(text) => {
            PluginError::WasmError(format!("plugin '{id}' component compile failed: {text}"))
        }
        other => other,
    })?;
    let linker = new_component_linker(&engine)?;
    let pre = linker
        .instantiate_pre(&component)
        .map_err(|e| wasm_err(&format!("plugin '{id}' pre-instantiation failed"), e))?;
    let stats = Arc::new(WasmStats::default());
    let pool = ComponentSessionPool::new(&id, &engine, &pre, grants, limits, &stats);
    let inner = ComponentPluginInner {
        manifest: manifest.clone(),
        engine,
        limits: limits.clone(),
        decl: std::sync::RwLock::new(WasmContributionDecl::default()),
        stats: stats.clone(),
        pool,
    };
    let decl = fetch_component_declaration(&inner).await?;
    *wf_common::lock::write_ok(inner.decl.write()) = decl;
    Ok(Arc::new(ComponentPlugin::from_inner(inner)) as Arc<dyn PluginTrait>)
}

fn component_cache() -> &'static moka::sync::Cache<String, Component> {
    static CACHE: std::sync::OnceLock<moka::sync::Cache<String, Component>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(pool::MAX_CACHED_ARTIFACTS)
            .build()
    })
}

/// Compile component bytes, reusing a content-keyed bounded cache so
/// reloads of an unchanged plugin skip recompilation.
pub fn cached_component(engine: &Engine, bytes: &[u8]) -> PluginResult<Component> {
    let digest = blake3::hash(bytes).to_hex().to_string();
    if let Some(component) = component_cache().get(&digest) {
        return Ok(component);
    }
    let component =
        Component::new(engine, bytes).map_err(|e| wasm_err("component compile failed", e))?;
    component_cache().insert(digest, component.clone());
    Ok(component)
}

/// Call the component `dispatch` export. Required whenever the plugin
/// declares contributions; a guest `Err` becomes a plugin error. The
/// session is checked out from the pool and returned afterwards; only
/// sessions from successful calls are eligible for reuse.
async fn invoke_component_dispatch(
    inner: &Arc<ComponentPluginInner>,
    handler_type: &str,
    handler_name: &str,
    input_json: &str,
) -> PluginResult<Vec<u8>> {
    let inner_clone = inner.clone();
    let handler_type = handler_type.to_owned();
    let handler_name = handler_name.to_owned();
    let input_json = input_json.to_owned();
    let mut session = inner.pool.acquire().await?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let contributions = session.bindings.wf_plugin_contributions();
    let fuel_before = component_fuel_before(inner, &session.store);
    let call = async {
        let result = contributions
            .call_dispatch(
                &mut session.store,
                &handler_type,
                &handler_name,
                &input_json,
            )
            .await
            .map_err(|e| component_call_error(&inner_clone, "dispatch", e))?;
        match result {
            Ok(output) => Ok(output.into_bytes()),
            Err(e) => Err(PluginError::WasmError(format!(
                "plugin '{}' dispatch failed: {e}",
                inner_clone.manifest.id
            ))),
        }
    };
    let outcome = guard_component_call(&inner.manifest.id, &inner.limits, &inner.stats, call).await;
    record_component_fuel(inner, &session.store, fuel_before);
    drain_component_stdio(inner, "dispatch", &session.store);
    inner.pool.release(session, outcome.is_ok()).await;
    outcome
}

fn config_json(config: &Value) -> PluginResult<String> {
    serde_json::to_string(config)
        .map_err(|e| PluginError::WasmError(format!("hook input serialize failed: {e}")))
}

#[async_trait]
impl PluginTrait for ComponentPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.inner.manifest
    }

    async fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        let config = config_json(&ctx.config)?;
        invoke_component_hook(
            &self.inner,
            HookOp::Load,
            Some(HookInput {
                plugin_id: self.inner.manifest.id.clone(),
                config,
            }),
        )
        .await
    }

    async fn on_unload(&self, _ctx: &PluginContext) -> PluginResult<()> {
        invoke_component_hook(&self.inner, HookOp::Unload, None).await
    }

    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> {
        let config = config_json(&ctx.config)?;
        invoke_component_hook(
            &self.inner,
            HookOp::Activate,
            Some(HookInput {
                plugin_id: self.inner.manifest.id.clone(),
                config,
            }),
        )
        .await
    }

    async fn on_deactivate(&self, _ctx: &PluginContext) -> PluginResult<()> {
        invoke_component_hook(&self.inner, HookOp::Deactivate, None).await
    }

    async fn on_config_change(&self, config: &Value) -> PluginResult<()> {
        let config = config_json(config)?;
        invoke_component_hook(
            &self.inner,
            HookOp::ConfigChange,
            Some(HookInput {
                plugin_id: self.inner.manifest.id.clone(),
                config,
            }),
        )
        .await
    }

    /// Re-call `register` and adopt the result when it differs from the
    /// cached declaration. Returns true exactly when the engine must
    /// re-sync contributions.
    async fn reload_declaration(&self) -> PluginResult<bool> {
        let fresh = fetch_component_declaration(&self.inner).await?;
        let mut current = wf_common::lock::write_ok(self.inner.decl.write());
        if *current == fresh {
            return Ok(false);
        }
        *current = fresh;
        Ok(true)
    }

    /// The six registration loops intentionally mirror the core-module path;
    /// see the matching note there. Shared call logic lives in
    /// `super::shared`.
    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        let decl = wf_common::lock::read_ok(self.inner.decl.read());
        for name in &decl.node_types {
            registrar.register_node_type(
                name,
                Arc::new(ComponentNodeHandler {
                    inner: self.inner.clone(),
                    type_name: name.clone(),
                }),
            )?;
        }
        for name in &decl.tool_types {
            registrar.register_tool_type(
                name,
                Arc::new(ComponentToolExecutor {
                    inner: self.inner.clone(),
                    type_name: name.clone(),
                }),
            )?;
        }
        for name in &decl.llm_providers {
            registrar.register_llm_provider(
                name,
                Arc::new(ComponentLlmCodec {
                    inner: self.inner.clone(),
                    name: name.clone(),
                }),
            )?;
        }
        for event_type in &decl.event_handlers {
            registrar.register_event_handler(
                event_type,
                Arc::new(ComponentEventHandler {
                    inner: self.inner.clone(),
                    event_type: event_type.clone(),
                }),
            )?;
        }
        super::warn_unknown_middleware_phases(&self.inner.manifest.id, &decl.middleware);
        for mw in &decl.middleware {
            let phase = MiddlewarePhase::from(mw.phase.as_str());
            registrar.register_middleware(
                phase,
                mw.priority,
                Arc::new(ComponentMiddlewareHandler {
                    inner: self.inner.clone(),
                    phase: mw.phase.clone(),
                }),
            )?;
        }
        Ok(())
    }
}

pub struct ComponentNodeHandler {
    inner: Arc<ComponentPluginInner>,
    type_name: String,
}

pub struct ComponentToolExecutor {
    inner: Arc<ComponentPluginInner>,
    type_name: String,
}

pub struct ComponentLlmCodec {
    inner: Arc<ComponentPluginInner>,
    name: String,
}

impl ComponentLlmCodec {
    /// Structured codec round-trip over the component dispatch channel
    /// (sync wrapper over the async guest call; see `WasmLlmCodec`).
    fn roundtrip<T: serde::de::DeserializeOwned>(&self, op: &str, input: Value) -> PluginResult<T> {
        let input_json = super::shared::encode_call_input(&input, "llm codec input")?;
        let inner = self.inner.clone();
        let handler = format!("{}/{}", self.name, op);
        let output = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(invoke_component_dispatch(
                    &inner,
                    "llm-codec",
                    &handler,
                    &input_json,
                ))
            })?,
            Err(_) => {
                return Err(PluginError::WasmError(
                    "llm codec requires a tokio runtime".to_string(),
                ));
            }
        };
        super::shared::decode_call_output(&output, "llm codec output")
    }
}

impl PluginLlmCodec for ComponentLlmCodec {
    fn build_request(&self, request: Value, profile: Value) -> PluginResult<CodecHttpRequest> {
        self.roundtrip(
            "build_request",
            serde_json::json!({"request": request, "profile": profile}),
        )
    }

    fn parse_response(&self, body: &str, request: Value) -> PluginResult<Value> {
        self.roundtrip(
            "parse_response",
            serde_json::json!({"body": body, "request": request}),
        )
    }

    fn parse_stream_chunk(&self, chunk: &str) -> PluginResult<Option<Value>> {
        self.roundtrip("parse_stream_chunk", serde_json::json!({"chunk": chunk}))
    }

    fn convert_tools(&self, tools: Value) -> PluginResult<Value> {
        self.roundtrip("convert_tools", serde_json::json!({"tools": tools}))
    }

    fn parse_tool_calls(&self, result: Value) -> PluginResult<Value> {
        self.roundtrip("parse_tool_calls", serde_json::json!({"result": result}))
    }

    fn build_count_tokens_request(
        &self,
        request: Value,
        profile: Value,
    ) -> PluginResult<Option<CodecHttpRequest>> {
        self.roundtrip(
            "build_count_tokens_request",
            serde_json::json!({"request": request, "profile": profile}),
        )
    }

    fn parse_count_tokens_response(&self, body: Value) -> PluginResult<u32> {
        self.roundtrip(
            "parse_count_tokens_response",
            serde_json::json!({"body": body}),
        )
    }
}

pub struct ComponentEventHandler {
    inner: Arc<ComponentPluginInner>,
    event_type: String,
}

pub struct ComponentMiddlewareHandler {
    inner: Arc<ComponentPluginInner>,
    phase: String,
}

#[async_trait]
impl PluginNodeHandler for ComponentNodeHandler {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        let input = super::shared::encode_call_input(&ctx, "node ctx")?;
        let output =
            invoke_component_dispatch(&self.inner, "node", &self.type_name, &input).await?;
        super::shared::decode_call_output(&output, "node result")
    }
}

#[async_trait]
impl PluginToolExecutor for ComponentToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let input = super::shared::encode_call_input(&ctx, "tool ctx")?;
        let output =
            invoke_component_dispatch(&self.inner, "tool", &self.type_name, &input).await?;
        super::shared::decode_call_output(&output, "tool result")
    }
}

#[async_trait]
impl PluginEventHandler for ComponentEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let input = super::shared::encode_call_input(&event, "event")?;
        invoke_component_dispatch(&self.inner, "event", &self.event_type, &input).await?;
        Ok(())
    }
}

#[async_trait]
impl PluginMiddlewareHandler for ComponentMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
        let input = super::shared::encode_call_input(&context, "middleware ctx")?;
        let output = invoke_component_dispatch(&self.inner, "mw", &self.phase, &input).await?;
        super::shared::resolve_middleware_output(&output, &context, next).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::loader::is_component;
    use super::super::policy::{resolve_grants, resolve_limits};
    use super::*;
    use crate::contributions::ContributionManager;
    use crate::manifest::PluginType;

    const TEST_WIT: &str = include_str!("../../wit/plugin.wit");

    /// Hand-written core-module guest implementing the `wf:plugin/plugin`
    /// world with canonical-ABI exports. `register` declares three tools;
    /// `dispatch` routes on the handler name: `echo` returns a fixed tool
    /// result, `boom` returns a guest error, `spin` never returns.
    /// Lifecycle hooks always succeed.
    ///
    /// Canonical ABI note: exported functions with more flat results than
    /// fit in one value return a single pointer to a guest-written return
    /// area instead, so every function below writes its results to the
    /// static area at `RET` and returns `RET`.
    const TEST_GUEST_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 8192))
  (global $ret i32 (i32.const 2048))
  (data (i32.const 0) "echo")
  (data (i32.const 8) "boom")
  (data (i32.const 16) "spin")
  (data (i32.const 32) "{\"result\":{\"echo\":true}}")
  (data (i32.const 64) "boom failed")
  (data (i32.const 128) "\00\00\00\00\04\00\00\00\08\00\00\00\04\00\00\00\10\00\00\00\04\00\00\00")
  (func (export "cabi_realloc")
    (param $old i32) (param $old_size i32) (param $align i32) (param $new_size i32)
    (result i32)
    (local $p i32)
    (global.set $heap
      (i32.and
        (i32.add (global.get $heap) (i32.sub (local.get $align) (i32.const 1)))
        (i32.xor (i32.sub (local.get $align) (i32.const 1)) (i32.const -1))))
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $new_size)))
    (local.get $p))
  (func $streq (param $p1 i32) (param $l1 i32) (param $p2 i32) (param $l2 i32) (result i32)
    (local $i i32)
    (if (i32.ne (local.get $l1) (local.get $l2))
      (then (i32.const 0) (return)))
    (local.set $i (i32.const 0))
    (block $done
      (loop $cmp
        (br_if $done (i32.ge_u (local.get $i) (local.get $l1)))
        (if (i32.ne
              (i32.load8_u (i32.add (local.get $p1) (local.get $i)))
              (i32.load8_u (i32.add (local.get $p2) (local.get $i))))
          (then (i32.const 0) (return)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $cmp)))
    (i32.const 1))
  (func $ok (result i32)
    (i32.store (global.get $ret) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 0))
    (global.get $ret))
  (func (export "wf:plugin/lifecycle@0.1.0-draft#on-load")
    (param i32 i32 i32 i32) (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@0.1.0-draft#on-activate")
    (param i32 i32 i32 i32) (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@0.1.0-draft#on-deactivate")
    (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@0.1.0-draft#on-unload")
    (result i32)
    (call $ok))
  (func (export "wf:plugin/lifecycle@0.1.0-draft#on-config-change")
    (param i32 i32) (result i32)
    (call $ok))
  (func (export "wf:plugin/contributions@0.1.0-draft#register")
    (result i32)
    (i32.store (global.get $ret) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 128))
    (i32.store (i32.add (global.get $ret) (i32.const 12)) (i32.const 3))
    (i32.store (i32.add (global.get $ret) (i32.const 16)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 20)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 24)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 28)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 32)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 36)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 40)) (i32.const 0))
    (i32.store (i32.add (global.get $ret) (i32.const 44)) (i32.const 0))
    (global.get $ret))
  (func (export "wf:plugin/contributions@0.1.0-draft#dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32)
    (result i32)
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 16) (i32.const 4))
      (then (loop $spin (br $spin))))
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 8) (i32.const 4))
      (then
        (i32.store (global.get $ret) (i32.const 1))
        (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 64))
        (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 11)))
      (else
        (i32.store (global.get $ret) (i32.const 0))
        (i32.store (i32.add (global.get $ret) (i32.const 4)) (i32.const 32))
        (i32.store (i32.add (global.get $ret) (i32.const 8)) (i32.const 24))))
    (global.get $ret)))
"#;

    fn write_u32_leb(value: u32, out: &mut Vec<u8>) {
        let mut v = value;
        loop {
            let mut byte = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                break;
            }
        }
    }

    /// Embed a `metadata::encode` fragment as one opaque
    /// `component-type:*` custom section right after the module header.
    /// The fragment is itself a tiny component carrying the world type,
    /// which is exactly what the encoder decodes back.
    fn embed_component_type(module: &[u8], section_name: &str, fragment: &[u8]) -> Vec<u8> {
        let mut inner = Vec::new();
        write_u32_leb(section_name.len() as u32, &mut inner);
        inner.extend_from_slice(section_name.as_bytes());
        inner.extend_from_slice(fragment);
        let mut section = vec![0u8];
        write_u32_leb(inner.len() as u32, &mut section);
        section.extend_from_slice(&inner);
        let mut full = module.to_vec();
        full.splice(8..8, section);
        full
    }

    /// Encode a core-module WAT guest into a component binary for the
    /// given WIT world: parse WIT, embed the world type, then run the
    /// component encoder.
    fn encode_component_with_wit(
        core_wat: &str,
        wit: &str,
        wit_path: &str,
        world: &str,
    ) -> Vec<u8> {
        let module = wat::parse_str(core_wat).expect("valid core guest wat");
        let mut resolve = wit_parser::Resolve::new();
        let pkg = resolve.push_str(wit_path, wit).expect("wit parses");
        let world = resolve
            .select_world(&[pkg], Some(world))
            .expect("world resolves");
        let fragment = wit_component::metadata::encode(
            &resolve,
            world,
            wit_component::StringEncoding::UTF8,
            None,
        )
        .expect("metadata encodes");
        let full = embed_component_type(&module, "component-type:plugin", &fragment);
        wit_component::ComponentEncoder::default()
            .module(&full)
            .expect("encoder accepts module")
            .validate(true)
            .encode()
            .expect("component encodes")
    }

    /// Encode the standard test guest for the `wf:plugin/plugin` world.
    fn encode_test_component(core_wat: &str) -> Vec<u8> {
        encode_component_with_wit(core_wat, TEST_WIT, "plugin.wit", "plugin")
    }

    fn test_manifest(id: &str, wasm: wf_plugin_sdk::manifest::WasmConfig) -> PluginManifest {
        PluginManifest {
            id: id.into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Wasm),
            sdk_version: None,
            entry_point: "plugin.wasm".into(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            wasm: Some(wasm),
            lua: None,
        }
    }

    /// Load a component through the real load path (size check, compile
    /// cache, pre-resolve, declaration fetch), mirroring the loader.
    async fn load_test_component(
        id: &str,
        bytes: &[u8],
        wasm: wf_plugin_sdk::manifest::WasmConfig,
        guard_timeout_ms: u64,
    ) -> PluginResult<Arc<dyn PluginTrait>> {
        let manifest = test_manifest(id, wasm);
        let limits = resolve_limits(&manifest, guard_timeout_ms);
        let grants = resolve_grants(&manifest);
        load_component_plugin_at(&manifest, bytes, &limits, &grants).await
    }

    fn hook_context(plugin_id: &str) -> PluginContext {
        PluginContext {
            plugin_id: plugin_id.to_owned(),
            sdk_version: "0.1.0".into(),
            config: Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: Arc::new(ContributionManager::new()),
        }
    }

    #[tokio::test]
    async fn component_cache_is_bounded() {
        // Distinct guests keep the encoded bytes distinct: the replacement
        // preserves the 11-byte payload length the guest hardcodes.
        let mut resolve = wit_parser::Resolve::new();
        let pkg = resolve
            .push_str("plugin.wit", TEST_WIT)
            .expect("wit parses");
        let world = resolve
            .select_world(&[pkg], Some("plugin"))
            .expect("world resolves");
        let engine = super::super::pool::engine_handle();
        let cap = super::super::pool::MAX_CACHED_ARTIFACTS;
        for i in 0..(cap + 8) {
            let core_wat = TEST_GUEST_WAT.replace("boom failed", &format!("boomfail{i:03}"));
            let module = wat::parse_str(&core_wat).expect("valid core guest wat");
            let fragment = wit_component::metadata::encode(
                &resolve,
                world,
                wit_component::StringEncoding::UTF8,
                None,
            )
            .expect("metadata encodes");
            let full = embed_component_type(&module, "component-type:plugin", &fragment);
            let bytes = wit_component::ComponentEncoder::default()
                .module(&full)
                .expect("encoder accepts module")
                .validate(true)
                .encode()
                .expect("component encodes");
            cached_component(&engine, &bytes).expect("component caches");
        }
        super::component_cache().run_pending_tasks();
        assert!(
            super::component_cache().entry_count() <= cap,
            "cache holds {} entries, cap is {cap}",
            super::component_cache().entry_count()
        );
    }

    #[test]
    fn component_wit_version_is_pinned() {
        assert!(
            TEST_WIT.contains(WF_COMPONENT_WORLD_VERSION),
            "wit world must carry the pinned version {WF_COMPONENT_WORLD_VERSION}"
        );
        assert!(
            TEST_WIT.contains("wf:plugin"),
            "wit world must declare the wf:plugin package"
        );
    }
    #[test]
    fn encoded_guest_is_detected_as_component() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        assert!(is_component(&bytes));
    }
    #[test]
    fn encoded_guest_exports_plugin_world() {
        use wasmparser::{Parser, Payload};

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let mut names = Vec::new();
        for payload in Parser::new(0).parse_all(&bytes) {
            if let Payload::ComponentExportSection(reader) = payload.expect("parses") {
                for export in reader {
                    names.push(export.expect("export").name.0.to_string());
                }
            }
        }
        assert!(
            names.contains(&"wf:plugin/lifecycle@0.1.0-draft".to_string()),
            "exports: {names:?}"
        );
        assert!(
            names.contains(&"wf:plugin/contributions@0.1.0-draft".to_string()),
            "exports: {names:?}"
        );
    }

    #[tokio::test]
    async fn component_full_lifecycle_end_to_end() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component("comp-e2e", &bytes, Default::default(), 10_000)
            .await
            .expect("component loads");

        let ctx = hook_context("comp-e2e");
        plugin.on_load(&ctx).await.expect("on-load");
        plugin.on_activate(&ctx).await.expect("on-activate");

        let manager = ContributionManager::new();
        manager.start_registration("comp-e2e");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("echo")
            .expect("echo tool registered");
        let result = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect("dispatch");
        assert_eq!(result.result, serde_json::json!({"echo": true}));

        plugin.on_deactivate(&ctx).await.expect("on-deactivate");
        plugin.on_unload(&ctx).await.expect("on-unload");

        // Every lifecycle hook plus the tool dispatch succeeded; a repeat
        // dispatch over the same plugin keeps working.
        let again = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect("second dispatch");
        assert_eq!(again.result, serde_json::json!({"echo": true}));
    }

    #[tokio::test]
    async fn component_dispatch_error_surfaces_as_wasm_error() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component("comp-boom", &bytes, Default::default(), 10_000)
            .await
            .expect("component loads");

        let manager = ContributionManager::new();
        manager.start_registration("comp-boom");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("boom")
            .expect("boom tool registered");
        let err = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect_err("guest Err must fail");
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected wasm error, got {err:?}"
        );
        let text = err.to_string();
        assert!(
            text.contains("comp-boom"),
            "error must name the plugin: {text}"
        );
        assert!(
            text.contains("boom failed"),
            "guest detail must surface: {text}"
        );
    }

    /// The standard test guest answers dispatch with a fixed JSON object
    /// (neither a boolean nor a middleware envelope), so the middleware
    /// handler must pass the context through unchanged (legacy behavior).
    #[tokio::test]
    async fn component_middleware_legacy_output_passes_context_through() {
        use crate::contributions::NextFn;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let engine = super::super::pool::engine_handle();
        let component = cached_component(&engine, &bytes).expect("compiles");
        let linker = new_component_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&component).expect("pre-instantiate");
        let manifest = test_manifest("comp-mw", Default::default());
        let limits = resolve_limits(&manifest, 10_000);
        let grants = resolve_grants(&manifest);
        let stats = Arc::new(WasmStats::default());
        let pool = ComponentSessionPool::new("comp-mw", &engine, &pre, &grants, &limits, &stats);
        let inner = Arc::new(ComponentPluginInner {
            manifest,
            engine,
            limits,
            decl: std::sync::RwLock::new(WasmContributionDecl::default()),
            stats,
            pool,
        });
        let handler = ComponentMiddlewareHandler {
            inner,
            phase: "pre_tool".to_owned(),
        };
        let input = serde_json::json!({"a": 1});
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let out = handler
            .handle(input.clone(), next)
            .await
            .expect("middleware runs");
        assert_eq!(out, input);
    }

    /// Test guest variant that calls the `host-log` world import from
    /// `on-load` before succeeding. The import line matches the world
    /// import name; the call passes `(level, ptr, len)` of a static message.
    fn host_log_component_wat() -> String {
        TEST_GUEST_WAT
            .replacen(
                "(module",
                "(module\n  (import \"wf:plugin/host-log@0.1.0-draft\" \"log\" (func $host_log (param i32 i32 i32)))\n  (data (i32.const 4096) \"hello-host\")",
                1,
            )
            .replacen(
                "(func (export \"wf:plugin/lifecycle@0.1.0-draft#on-load\")\n    (param i32 i32 i32 i32) (result i32)\n    (call $ok))",
                "(func (export \"wf:plugin/lifecycle@0.1.0-draft#on-load\")\n    (param i32 i32 i32 i32) (result i32)\n    (call $host_log (i32.const 2) (i32.const 4096) (i32.const 10))\n    (call $ok))",
                1,
            )
    }

    #[tokio::test]
    async fn component_host_log_import_resolves_and_runs() {
        let bytes = encode_test_component(&host_log_component_wat());
        let plugin = load_test_component("comp-hostlog", &bytes, Default::default(), 10_000)
            .await
            .expect("component with host-log import loads");
        plugin
            .on_load(&hook_context("comp-hostlog"))
            .await
            .expect("host log call succeeds");
    }

    #[tokio::test]
    async fn component_register_call_records_fuel() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let engine = super::super::pool::engine_handle();
        let component = cached_component(&engine, &bytes).expect("compiles");
        let linker = new_component_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&component).expect("pre-instantiate");
        let manifest = test_manifest("comp-fuel", Default::default());
        let limits = resolve_limits(&manifest, 10_000);
        let grants = resolve_grants(&manifest);
        let stats = Arc::new(WasmStats::default());
        let pool = ComponentSessionPool::new("comp-fuel", &engine, &pre, &grants, &limits, &stats);
        let inner = ComponentPluginInner {
            manifest,
            engine,
            limits,
            decl: std::sync::RwLock::new(WasmContributionDecl::default()),
            stats: stats.clone(),
            pool,
        };
        fetch_component_declaration(&inner)
            .await
            .expect("register works");
        let snap = stats.snapshot();
        assert!(snap.calls >= 1, "register call is counted");
        assert!(
            snap.fuel_consumed > 0,
            "metered component calls consume fuel, got {snap:?}"
        );
    }

    #[tokio::test]
    async fn component_spin_guest_times_out() {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component(
            "comp-spin",
            &bytes,
            WasmConfig {
                fuel_limit: Some(0),
                call_timeout_ms: Some(200),
                ..Default::default()
            },
            0,
        )
        .await
        .expect("component loads");

        let manager = ContributionManager::new();
        manager.start_registration("comp-spin");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("spin")
            .expect("spin tool registered");
        let err = executor
            .execute(PluginToolContext {
                args: serde_json::json!({}),
            })
            .await
            .expect_err("spinning guest must time out");
        assert!(
            matches!(err, PluginError::Timeout { .. }),
            "expected timeout, got {err:?}"
        );
    }

    #[tokio::test]
    async fn component_pool_reuses_idle_session() {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let engine = super::super::pool::engine_handle();
        let component = cached_component(&engine, &bytes).expect("compiles");
        let linker = new_component_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&component).expect("pre-instantiate");
        let manifest = test_manifest(
            "comp-pool",
            WasmConfig {
                store_pool_size: Some(2),
                ..Default::default()
            },
        );
        let limits = resolve_limits(&manifest, 10_000);
        let grants = resolve_grants(&manifest);
        let stats = Arc::new(WasmStats::default());
        let pool = ComponentSessionPool::new("comp-pool", &engine, &pre, &grants, &limits, &stats);
        assert!(pool.enabled());

        let session = pool.acquire().await.expect("first acquire builds");
        assert_eq!(stats.snapshot().pool_misses, 1);
        pool.release(session, true).await;
        let reused = pool.acquire().await.expect("second acquire reuses");
        assert_eq!(stats.snapshot().pool_hits, 1);
        pool.release(reused, false).await;
        assert_eq!(stats.snapshot().pool_drops, 1);
    }

    #[tokio::test]
    async fn component_pooled_dispatch_roundtrips() {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component(
            "comp-pooled-e2e",
            &bytes,
            WasmConfig {
                store_pool_size: Some(2),
                ..Default::default()
            },
            10_000,
        )
        .await
        .expect("component loads");

        let manager = ContributionManager::new();
        manager.start_registration("comp-pooled-e2e");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        let executor = manager
            .get_tool_executor("echo")
            .expect("echo tool registered");
        for _ in 0..2 {
            let result = executor
                .execute(PluginToolContext {
                    args: serde_json::json!({}),
                })
                .await
                .expect("pooled dispatch");
            assert_eq!(result.result, serde_json::json!({"echo": true}));
        }
    }

    #[tokio::test]
    async fn component_reload_reports_no_change_for_static_guest() {
        let bytes = encode_test_component(TEST_GUEST_WAT);
        let plugin = load_test_component("comp-reload", &bytes, Default::default(), 10_000)
            .await
            .expect("component loads");
        assert!(!plugin.reload_declaration().await.expect("reload"));
    }

    #[tokio::test]
    async fn component_missing_exports_rejected() {
        // A component from an empty world carries no lifecycle or
        // contribution exports, so host binding must fail at load.
        let bytes = encode_component_with_wit(
            r#"(module
  (memory (export "memory") 1)
  (func (export "cabi_realloc")
    (param i32 i32 i32 i32) (result i32)
    (i32.const 1024)))
"#,
            "package wf:empty@0.1.0;\nworld hollow {\n}\n",
            "empty.wit",
            "hollow",
        );
        assert!(is_component(&bytes));

        let err = match load_test_component("comp-hollow", &bytes, Default::default(), 10_000).await
        {
            Ok(_) => panic!("exports missing, load must fail"),
            Err(e) => e,
        };
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected bind failure, got {err:?}"
        );
    }
}
