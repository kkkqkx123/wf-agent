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
use wasmtime::component::{Component, Instance, InstancePre, Linker, ResourceTable};
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
        async: true,
    });
}

use gen::exports::wf::plugin::lifecycle::HookInput;
/// Type alias for the generated world struct, avoiding name clashes.
use gen::Plugin as GeneratedPlugin;

/// WASI p2 view for component guests: context plus the resource table and
/// the memory limiter. Capabilities default to closed; grants are applied
/// per plugin at session build time.
pub(crate) struct ComponentHostState {
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

/// Instantiate the component asynchronously. The shared engine enables
/// async support, so sync instantiation panics; instantiation itself runs
/// no guest logic beyond an optional start function, so it stays on the
/// calling task while the actual guest call runs on a blocking thread.
async fn instantiate_component(
    inner: &ComponentPluginInner,
    store: &mut Store<ComponentHostState>,
) -> PluginResult<Instance> {
    inner.pre.instantiate_async(&mut *store).await.map_err(|e| {
        wasm_err(
            &format!("plugin '{}' instantiate failed", inner.manifest.id),
            e,
        )
    })
}

/// Bind an instantiated component to the generated host imports. The
/// component exports the full world, so unlike the tolerant core-module
/// path a bind failure here means host/guest skew and fails loudly.
fn bind_instance(
    inner: &ComponentPluginInner,
    store: &mut Store<ComponentHostState>,
    instance: &Instance,
) -> PluginResult<GeneratedPlugin> {
    GeneratedPlugin::new(store, instance)
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
    let name = op.name();
    let mut store = build_component_store(
        &inner.engine,
        &inner.manifest.id,
        &inner.grants,
        &inner.limits,
    )?;
    pool::arm_epoch(&inner.engine, &mut store, inner.limits.call_timeout_ms);
    let instance = instantiate_component(inner, &mut store).await?;
    let bindings = bind_instance(inner, &mut store, &instance)?;
    let lifecycle = bindings.wf_plugin_lifecycle();
    let call = async {
        let result = match (op, input) {
            (HookOp::Load, Some(input)) => lifecycle.call_on_load(&mut store, &input).await,
            (HookOp::Activate, Some(input)) => lifecycle.call_on_activate(&mut store, &input).await,
            (HookOp::Deactivate, None) => lifecycle.call_on_deactivate(&mut store).await,
            (HookOp::Unload, None) => lifecycle.call_on_unload(&mut store).await,
            (HookOp::ConfigChange, Some(input)) => {
                lifecycle
                    .call_on_config_change(&mut store, &input.config)
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
        result.map_err(|e| PluginError::WasmError(format!("plugin hook '{name}' failed: {e}")))
    };
    guard_component_call(&inner.manifest.id, &inner.limits, &inner.stats, call).await
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
pub(crate) async fn fetch_component_declaration(
    inner: &ComponentPluginInner,
) -> PluginResult<WasmContributionDecl> {
    let limits = inner.limits.clone();
    let stats = inner.stats.clone();
    let plugin_id = inner.manifest.id.clone();
    let mut store = build_component_store(
        &inner.engine,
        &inner.manifest.id,
        &inner.grants,
        &inner.limits,
    )?;
    pool::arm_epoch(&inner.engine, &mut store, inner.limits.call_timeout_ms);
    let instance = instantiate_component(inner, &mut store).await?;
    let bindings =
        GeneratedPlugin::new(&mut store, &instance).map_err(|e| wasm_err("bind failed", e))?;
    let contributions = bindings.wf_plugin_contributions();
    let decl = guard_component_call(&plugin_id, &limits, &stats, async {
        contributions
            .call_register(&mut store)
            .await
            .map_err(|e| wasm_err("register call failed", e))
    })
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
    let mut store = build_component_store(
        &inner.engine,
        &inner.manifest.id,
        &inner.grants,
        &inner.limits,
    )?;
    pool::arm_epoch(&inner.engine, &mut store, inner.limits.call_timeout_ms);
    let instance = instantiate_component(inner, &mut store).await?;
    let bindings = bind_instance(inner, &mut store, &instance)?;
    let contributions = bindings.wf_plugin_contributions();
    let call = async {
        let result = contributions
            .call_dispatch(&mut store, &handler_type, &handler_name, &input_json)
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
    guard_component_call(&inner.manifest.id, &inner.limits, &inner.stats, call).await
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

    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        let decl = &self.inner.decl;
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
                Arc::new(ComponentLlmFormatter {
                    inner: self.inner.clone(),
                    name: name.clone(),
                }),
            )?;
        }
        for name in &decl.formatters {
            registrar.register_formatter(
                name,
                Arc::new(ComponentLlmFormatter {
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
        for mw in &decl.middleware {
            registrar.register_middleware(
                MiddlewarePhase::from(mw.phase.as_str()),
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
            wasm: Some(wasm),
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
        let dir =
            std::env::temp_dir().join(format!("wf-component-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("plugin.wasm");
        std::fs::write(&path, bytes).expect("write component");
        let manifest = test_manifest(id, wasm);
        let limits = resolve_limits(&manifest, guard_timeout_ms).expect("limits");
        let grants = resolve_grants(&manifest);
        load_component_plugin_at(&manifest, &path, &limits, &grants).await
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
            plugin.register_contributions(&mut registrar).expect("contributions register");
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
            plugin.register_contributions(&mut registrar).expect("contributions register");
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
            plugin.register_contributions(&mut registrar).expect("contributions register");
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
