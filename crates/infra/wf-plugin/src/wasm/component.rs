//! Component-model guest host (`wf:plugin/plugin` world).
//!
//! Dual-compat companion to the core-module path in `plugin.rs`: component
//! binaries (detected by magic in the loader) run through `bindgen!`
//! bindings with a WASI p2 context instead of the `wf_*` export contract.
//! The `Plugin` trait surface, contribution semantics, limits, and stats
//! stay identical, so callers never observe which guest kind they use.
//!
//! Sessions are synchronous inside `spawn_blocking` (mirroring the Lua
//! loader choice): component calls block a dedicated thread, never a tokio
//! worker. Fuel metering and epoch interruption apply unchanged.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use wasmtime::component::{Component, InstancePre, Linker, ResourceTable};
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView};
use wf_plugin_sdk::wasm::{WasmContributionDecl, WasmMiddlewareDecl};
use wf_types::MiddlewarePhase;

use super::policy::{WasiGrants, WasmLimits};
use super::pool::{self, wasm_err};
use super::stats::WasmStats;
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
    });
}

use gen::exports::wf::plugin::lifecycle::HookInput;
/// Type alias for the generated world struct, avoiding name clashes.
use gen::Plugin as GeneratedPlugin;

/// WASI p2 view for component guests: context plus the resource table and
/// the memory limiter. Capabilities default to closed; grants are applied
/// per plugin at session build time.
struct ComponentHostState {
    ctx: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
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

pub(crate) struct ComponentPluginInner {
    pub(crate) manifest: PluginManifest,
    pub(crate) engine: Engine,
    /// Pre-resolved component imports: per-call instantiation skips linker
    /// work, mirroring the core-module `InstancePre` path.
    pub(crate) pre: InstancePre<ComponentHostState>,
    pub(crate) limits: WasmLimits,
    pub(crate) grants: WasiGrants,
    pub(crate) decl: WasmContributionDecl,
    pub(crate) stats: Arc<WasmStats>,
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

/// Build the linker shared by every component plugin: WASI p2 imports only.
fn new_component_linker(engine: &Engine) -> PluginResult<Linker<ComponentHostState>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| wasm_err("component wasi linker setup failed", e))?;
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
    let mut builder = WasiCtxBuilder::new();
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
    let mut store = Store::new(
        engine,
        ComponentHostState {
            ctx: builder.build(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_max_bytes as usize)
                .build(),
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

/// Run one blocking component operation with the outer timeout backstop.
/// The epoch deadline (armed on the store before blocking) fires first for
/// spinning guests; this only guards host-side stalls. A blocking-thread
/// panic is reported as a plugin panic.
async fn run_blocking_raw<T>(
    plugin_id: &str,
    limits: &WasmLimits,
    stats: &WasmStats,
    op: &'static str,
    work: impl FnOnce() -> PluginResult<T> + Send + 'static,
) -> PluginResult<T>
where
    T: Send + 'static,
{
    let plugin_id = plugin_id.to_owned();
    let join = tokio::task::spawn_blocking(work);
    let outcome = match pool::outer_timeout_ms(limits.call_timeout_ms) {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), join)
            .await
            .map_err(|_| PluginError::Timeout {
                plugin_id: plugin_id.clone(),
            })?,
        None => join.await,
    };
    match outcome {
        Ok(result) => {
            let result = result?;
            stats.record(&Ok(()), None);
            Ok(result)
        }
        Err(join_err) => {
            let err = if join_err.is_panic() {
                PluginError::PluginPanic {
                    plugin_id: plugin_id.clone(),
                }
            } else {
                wasm_err(&format!("component call '{op}' join failed"), join_err)
            };
            stats.record::<()>(&Err(PluginError::WasmError(err.to_string())), None);
            Err(err)
        }
    }
}

/// Instantiate the component and bind its exports for one call.
fn bind_instance(
    inner: &ComponentPluginInner,
    store: &mut Store<ComponentHostState>,
) -> PluginResult<GeneratedPlugin> {
    let instance = inner.pre.instantiate(&mut *store).map_err(|e| {
        wasm_err(
            &format!("plugin '{}' instantiate failed", inner.manifest.id),
            e,
        )
    })?;
    GeneratedPlugin::new(store, &instance)
        .map_err(|e| wasm_err(&format!("plugin '{}' bind failed", inner.manifest.id), e))
}

/// Call one lifecycle hook. A component instantiates only when it
/// implements the full world, so unlike the tolerant core-module path a
/// bind failure here means host/guest skew and fails loudly.
async fn invoke_component_hook(
    inner: &Arc<ComponentPluginInner>,
    op: HookOp,
    input: Option<HookInput>,
) -> PluginResult<()> {
    let inner_clone = inner.clone();
    let name = op.name();
    run_blocking_raw(
        &inner.manifest.id,
        &inner.limits,
        &inner.stats,
        name,
        move || {
            let inner = &inner_clone;
            let mut store = build_component_store(
                &inner.engine,
                &inner.manifest.id,
                &inner.grants,
                &inner.limits,
            )?;
            pool::arm_epoch(&inner.engine, &mut store, inner.limits.call_timeout_ms);
            let bindings = bind_instance(inner, &mut store)?;
            let lifecycle = bindings.wf_plugin_lifecycle();
            let result = match (op, input) {
                (HookOp::OnLoad, Some(input)) => lifecycle.call_on_load(&mut store, &input),
                (HookOp::OnActivate, Some(input)) => lifecycle.call_on_activate(&mut store, &input),
                (HookOp::OnDeactivate, None) => lifecycle.call_on_deactivate(&mut store),
                (HookOp::OnUnload, None) => lifecycle.call_on_unload(&mut store),
                (HookOp::OnConfigChange, Some(input)) => {
                    lifecycle.call_on_config_change(&mut store, &input.config)
                }
                _ => {
                    return Err(PluginError::Internal(format!(
                        "hook '{}' called with wrong input shape",
                        op.name()
                    )));
                }
            }
            .map_err(|e| component_call_error(inner, name, e))?;
            result
                .map_err(|e| PluginError::WasmError(format!("plugin hook '{}' failed: {e}", name)))
        },
    )
    .await
}

#[derive(Debug, Clone, Copy)]
enum HookOp {
    OnLoad,
    OnActivate,
    OnDeactivate,
    OnUnload,
    OnConfigChange,
}

impl HookOp {
    fn name(self) -> &'static str {
        match self {
            HookOp::OnLoad => "on-load",
            HookOp::OnActivate => "on-activate",
            HookOp::OnDeactivate => "on-deactivate",
            HookOp::OnUnload => "on-unload",
            HookOp::OnConfigChange => "on-config-change",
        }
    }
}

/// Fetch the contribution declaration by calling `register` once at load.
pub(crate) async fn fetch_component_declaration(
    inner: &ComponentPluginInner,
) -> PluginResult<WasmContributionDecl> {
    let engine = inner.engine.clone();
    let pre = inner.pre.clone();
    let grants = inner.grants.clone();
    let limits = inner.limits.clone();
    let plugin_id = inner.manifest.id.clone();
    let stats = inner.stats.clone();
    let decl = {
        let pid = plugin_id.clone();
        let lim = limits.clone();
        run_blocking_raw(&plugin_id, &limits, &stats, "register", move || {
            let mut store = build_component_store(&engine, &pid, &grants, &lim)?;
            pool::arm_epoch(&engine, &mut store, lim.call_timeout_ms);
            let instance = pre
                .instantiate(&mut store)
                .map_err(|e| wasm_err(&format!("plugin '{pid}' instantiate failed"), e))?;
            let bindings = GeneratedPlugin::new(&mut store, &instance)
                .map_err(|e| wasm_err("bind failed", e))?;
            let decl = bindings
                .wf_plugin_contributions()
                .call_register(&mut store)
                .map_err(|e| wasm_err("register call failed", e))?;
            Ok(decl)
        })
    }
    .await?;
    Ok(WasmContributionDecl {
        node_types: decl.node_types,
        tool_types: decl.tool_types,
        llm_providers: decl.llm_providers,
        formatters: decl.formatters,
        event_handlers: decl.event_handlers,
        middleware: decl
            .middleware
            .into_iter()
            .map(|m| WasmMiddlewareDecl {
                phase: m.phase,
                priority: m.priority,
            })
            .collect(),
    })
}

/// Load a component-model plugin from its manifest and base directory.
/// Mirrors the core-module loader: path validation happens in `loader.rs`,
/// this function compiles, pre-resolves, and fetches the declaration.
pub(crate) async fn load_component_plugin_at(
    manifest: &PluginManifest,
    module_path: &std::path::Path,
    limits: &WasmLimits,
    grants: &WasiGrants,
) -> PluginResult<Arc<dyn PluginTrait>> {
    let id = manifest.id.clone();
    let bytes = tokio::fs::read(module_path)
        .await
        .map_err(|e| PluginError::LoadFailed(format!("cannot read {module_path:?}: {e}")))?;
    if bytes.len() as u64 > limits.max_module_bytes {
        return Err(PluginError::LoadFailed(format!(
            "wasm component {module_path:?} ({} bytes) exceeds limit of {} bytes",
            bytes.len(),
            limits.max_module_bytes
        )));
    }
    let engine = pool::engine_handle();
    let component = cached_component(&engine, &bytes).map_err(|e| match e {
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
    let inner = ComponentPluginInner {
        manifest: manifest.clone(),
        engine,
        pre,
        limits: limits.clone(),
        grants: grants.clone(),
        decl: WasmContributionDecl::default(),
        stats: stats.clone(),
    };
    let decl = fetch_component_declaration(&inner).await?;
    let inner = ComponentPluginInner { decl, ..inner };
    Ok(Arc::new(ComponentPlugin::from_inner(inner)) as Arc<dyn PluginTrait>)
}

fn component_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, Component>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Component>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Compile component bytes, reusing a content-keyed cache so reloads of an
/// unchanged plugin skip recompilation.
pub fn cached_component(engine: &Engine, bytes: &[u8]) -> PluginResult<Component> {
    let digest = blake3::hash(bytes).to_hex().to_string();
    if let Some(component) = component_cache()
        .lock()
        .expect("component cache poisoned")
        .get(&digest)
    {
        return Ok(component.clone());
    }
    let component =
        Component::new(engine, bytes).map_err(|e| wasm_err("component compile failed", e))?;
    component_cache()
        .lock()
        .expect("component cache poisoned")
        .insert(digest, component.clone());
    Ok(component)
}

/// Call the component `dispatch` export. Required whenever the plugin
/// declares contributions; a guest `Err` becomes a plugin error.
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
    run_blocking_raw(
        &inner.manifest.id,
        &inner.limits,
        &inner.stats,
        "dispatch",
        move || {
            let inner = &inner_clone;
            let mut store = build_component_store(
                &inner.engine,
                &inner.manifest.id,
                &inner.grants,
                &inner.limits,
            )?;
            pool::arm_epoch(&inner.engine, &mut store, inner.limits.call_timeout_ms);
            let bindings = bind_instance(inner, &mut store)?;
            let result = bindings
                .wf_plugin_contributions()
                .call_dispatch(&mut store, &handler_type, &handler_name, &input_json)
                .map_err(|e| component_call_error(inner, "dispatch", e))?;
            match result {
                Ok(output) => Ok(output.into_bytes()),
                Err(e) => Err(PluginError::WasmError(format!(
                    "plugin '{}' dispatch failed: {e}",
                    inner.manifest.id
                ))),
            }
        },
    )
    .await
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
            HookOp::OnLoad,
            Some(HookInput {
                plugin_id: self.inner.manifest.id.clone(),
                config,
            }),
        )
        .await
    }

    async fn on_unload(&self, _ctx: &PluginContext) -> PluginResult<()> {
        invoke_component_hook(&self.inner, HookOp::OnUnload, None).await
    }

    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> {
        let config = config_json(&ctx.config)?;
        invoke_component_hook(
            &self.inner,
            HookOp::OnActivate,
            Some(HookInput {
                plugin_id: self.inner.manifest.id.clone(),
                config,
            }),
        )
        .await
    }

    async fn on_deactivate(&self, _ctx: &PluginContext) -> PluginResult<()> {
        invoke_component_hook(&self.inner, HookOp::OnDeactivate, None).await
    }

    async fn on_config_change(&self, config: &Value) -> PluginResult<()> {
        let config = config_json(config)?;
        invoke_component_hook(
            &self.inner,
            HookOp::OnConfigChange,
            Some(HookInput {
                plugin_id: self.inner.manifest.id.clone(),
                config,
            }),
        )
        .await
    }

    fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
        let decl = &self.inner.decl;
        for name in &decl.node_types {
            registrar.register_node_type(
                name,
                Arc::new(ComponentNodeHandler {
                    inner: self.inner.clone(),
                    type_name: name.clone(),
                }),
            );
        }
        for name in &decl.tool_types {
            registrar.register_tool_type(
                name,
                Arc::new(ComponentToolExecutor {
                    inner: self.inner.clone(),
                    type_name: name.clone(),
                }),
            );
        }
        for name in &decl.llm_providers {
            registrar.register_llm_provider(
                name,
                Arc::new(ComponentLlmFormatter {
                    inner: self.inner.clone(),
                    name: name.clone(),
                }),
            );
        }
        for name in &decl.formatters {
            registrar.register_formatter(
                name,
                Arc::new(ComponentLlmFormatter {
                    inner: self.inner.clone(),
                    name: name.clone(),
                }),
            );
        }
        for event_type in &decl.event_handlers {
            registrar.register_event_handler(
                event_type,
                Arc::new(ComponentEventHandler {
                    inner: self.inner.clone(),
                    event_type: event_type.clone(),
                }),
            );
        }
        for mw in &decl.middleware {
            registrar.register_middleware(
                MiddlewarePhase::from(mw.phase.as_str()),
                mw.priority,
                Arc::new(ComponentMiddlewareHandler {
                    inner: self.inner.clone(),
                    phase: mw.phase.clone(),
                }),
            );
        }
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

pub struct ComponentLlmFormatter {
    inner: Arc<ComponentPluginInner>,
    name: String,
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
        let input = serde_json::to_string(&ctx)
            .map_err(|e| PluginError::WasmError(format!("serialize node ctx: {e}")))?;
        let output =
            invoke_component_dispatch(&self.inner, "node", &self.type_name, &input).await?;
        serde_json::from_slice::<PluginNodeResult>(&output)
            .map_err(|e| PluginError::WasmError(format!("deserialize node result: {e}")))
    }
}

#[async_trait]
impl PluginToolExecutor for ComponentToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let input = serde_json::to_string(&ctx)
            .map_err(|e| PluginError::WasmError(format!("serialize tool ctx: {e}")))?;
        let output =
            invoke_component_dispatch(&self.inner, "tool", &self.type_name, &input).await?;
        serde_json::from_slice::<PluginToolResult>(&output)
            .map_err(|e| PluginError::WasmError(format!("deserialize tool result: {e}")))
    }
}

#[async_trait]
impl PluginLlmFormatter for ComponentLlmFormatter {
    async fn format(&self, request: PluginLlmRequest) -> PluginResult<PluginLlmResponse> {
        let input = serde_json::to_string(&request)
            .map_err(|e| PluginError::WasmError(format!("serialize llm request: {e}")))?;
        let output = invoke_component_dispatch(&self.inner, "llm", &self.name, &input).await?;
        serde_json::from_slice::<PluginLlmResponse>(&output)
            .map_err(|e| PluginError::WasmError(format!("deserialize llm response: {e}")))
    }
}

#[async_trait]
impl PluginEventHandler for ComponentEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let input = serde_json::to_string(&event)
            .map_err(|e| PluginError::WasmError(format!("serialize event: {e}")))?;
        invoke_component_dispatch(&self.inner, "event", &self.event_type, &input).await?;
        Ok(())
    }
}

#[async_trait]
impl PluginMiddlewareHandler for ComponentMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<()> {
        let input = serde_json::to_string(&context)
            .map_err(|e| PluginError::WasmError(format!("serialize middleware ctx: {e}")))?;
        let output = invoke_component_dispatch(&self.inner, "mw", &self.phase, &input).await?;
        let proceed: Value = serde_json::from_slice(&output).unwrap_or(Value::Null);
        if proceed.as_bool().unwrap_or(true) {
            next().await?;
        }
        Ok(())
    }
}
