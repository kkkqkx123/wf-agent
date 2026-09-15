use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use wasmtime::Engine;
use wf_plugin_sdk::wasm::{export, WasmContributionDecl};
use wf_types::MiddlewarePhase;

use super::abi;
use super::policy::WasmLimits;
use super::pool::{self, PooledSession, SessionPool};
use super::stats::WasmStats;
use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::contributions::types::*;
use crate::contributions::NextFn;
use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub(crate) struct WasmPluginInner {
    pub(crate) manifest: PluginManifest,
    pub(crate) engine: Engine,
    pub(crate) limits: WasmLimits,
    pub(crate) decl: WasmContributionDecl,
    pub(crate) stats: Arc<WasmStats>,
    /// Idle session pool. Disabled by default; enabled only when the
    /// manifest requests it and the guest exports the heap-reset hook.
    /// The pool owns the pre-resolved imports and WASI grants, so every
    /// session it builds carries the same configuration.
    pub(crate) pool: SessionPool,
}

pub struct WasmPlugin {
    inner: Arc<WasmPluginInner>,
}

impl WasmPlugin {
    pub(crate) fn from_inner(inner: WasmPluginInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn declaration(&self) -> &WasmContributionDecl {
        &self.inner.decl
    }

    pub fn stats(&self) -> super::stats::WasmStatsSnapshot {
        self.inner.stats.snapshot()
    }
}

fn guest_call_error(
    inner: &WasmPluginInner,
    export_name: &str,
    err: wasmtime::Error,
) -> PluginError {
    let id = &inner.manifest.id;
    // Fuel exhaustion and epoch interruption surface as `Trap` in the error
    // chain while the top-level display shows only the wasm backtrace, so
    // classification must downcast instead of string-matching.
    if let Some(trap) = err.downcast_ref::<wasmtime::Trap>() {
        if matches!(trap, wasmtime::Trap::Interrupt) {
            return PluginError::Timeout {
                plugin_id: id.clone(),
            };
        }
        if matches!(trap, wasmtime::Trap::OutOfFuel) {
            return PluginError::WasmError(format!(
                "plugin '{id}' export '{export_name}' exhausted its fuel budget"
            ));
        }
    }
    pool::wasm_err(&format!("plugin '{id}' export '{export_name}' failed"), err)
}

/// Outer backstop for one guest call. The epoch deadline armed separately
/// should always fire first inside the guest.
async fn with_call_timeout<T, F>(inner: &WasmPluginInner, fut: F) -> PluginResult<T>
where
    F: std::future::Future<Output = PluginResult<T>>,
{
    match pool::outer_timeout_ms(inner.limits.call_timeout_ms) {
        Some(ms) => tokio::time::timeout(Duration::from_millis(ms), fut)
            .await
            .map_err(|_| PluginError::Timeout {
                plugin_id: inner.manifest.id.clone(),
            })?,
        None => fut.await,
    }
}

/// Attribute one finished guest call to the plugin counters, including
/// fuel consumption when the call runs under a budget.
fn observe<T>(inner: &WasmPluginInner, session: &PooledSession, result: &PluginResult<T>) {
    let fuel_used = inner.limits.fuel_limit.and_then(|budget| {
        session
            .store
            .get_fuel()
            .ok()
            .map(|remaining| budget.saturating_sub(remaining))
    });
    inner.stats.record(result, fuel_used);
}

/// Copy `input` into guest memory via the `alloc` export.
async fn write_input(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    input: &[u8],
) -> PluginResult<u32> {
    let id = &inner.manifest.id;
    let alloc = session
        .instance
        .get_typed_func::<u32, u32>(&mut session.store, export::ALLOC)
        .map_err(|_| {
            PluginError::WasmError(format!("plugin '{id}' does not export '{}'", export::ALLOC))
        })?;
    let ptr = with_call_timeout(inner, async {
        alloc
            .call_async(&mut session.store, input.len() as u32)
            .await
            .map_err(|e| guest_call_error(inner, export::ALLOC, e))
    })
    .await?;
    abi::write_bytes(&mut session.store, &session.memory, ptr, input)?;
    Ok(ptr)
}

/// Release a guest buffer when the guest provides `dealloc`. Best-effort:
/// the store is dropped after the call anyway, bounding any leak.
async fn release_output(_inner: &WasmPluginInner, session: &mut PooledSession, ptr: u32, len: u32) {
    let Ok(dealloc) = session
        .instance
        .get_typed_func::<(u32, u32), ()>(&mut session.store, export::DEALLOC)
    else {
        return;
    };
    let _ = dealloc.call_async(&mut session.store, (ptr, len)).await;
}

async fn read_output(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    packed: u64,
) -> PluginResult<Vec<u8>> {
    let (ptr, len) = abi::unpack_ptr_len(packed);
    let bytes = abi::read_bytes(&session.store, &session.memory, ptr, len)?;
    release_output(inner, session, ptr, len).await;
    Ok(bytes)
}

/// Call a lifecycle hook export with a JSON envelope. A missing export
/// counts as success, matching the Lua loader convention. The session is
/// checked out from the pool and returned afterwards; only sessions from
/// successful calls are eligible for reuse.
async fn invoke_hook(inner: &WasmPluginInner, export_name: &str, input: &[u8]) -> PluginResult<()> {
    let mut session = inner.pool.acquire().await?;
    let result = invoke_hook_inner(inner, &mut session, export_name, input).await;
    observe(inner, &session, &result);
    inner.pool.release(session, result.is_ok()).await;
    result
}

async fn invoke_hook_inner(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    export_name: &str,
    input: &[u8],
) -> PluginResult<()> {
    if session
        .instance
        .get_func(&mut session.store, export_name)
        .is_none()
    {
        tracing::debug!(
            "wasm plugin '{}' has no export '{}'",
            inner.manifest.id,
            export_name
        );
        return Ok(());
    }
    let hook = session
        .instance
        .get_typed_func::<(u32, u32), u32>(&mut session.store, export_name)
        .map_err(|e| {
            pool::wasm_err(
                &format!(
                    "plugin '{}' export '{export_name}' has an unexpected signature",
                    inner.manifest.id
                ),
                e,
            )
        })?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let ptr = write_input(inner, &mut *session, input).await?;
    let code = with_call_timeout(inner, async {
        hook.call_async(&mut session.store, (ptr, input.len() as u32))
            .await
            .map_err(|e| guest_call_error(inner, export_name, e))
    })
    .await;
    let code = code?;
    release_output(inner, &mut *session, ptr, input.len() as u32).await;
    abi::check_hook_status(export_name, &inner.manifest.id, code)
}

/// Fetch the contribution declaration by calling `wf_register`.
/// Used once at load time; the result is cached on the plugin.
pub(crate) async fn fetch_declaration(
    inner: &WasmPluginInner,
) -> PluginResult<WasmContributionDecl> {
    let mut session = inner.pool.acquire().await?;
    let result = fetch_declaration_inner(inner, &mut session).await;
    observe(inner, &session, &result);
    inner.pool.release(session, result.is_ok()).await;
    result
}

async fn fetch_declaration_inner(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
) -> PluginResult<WasmContributionDecl> {
    if session
        .instance
        .get_func(&mut session.store, export::REGISTER)
        .is_none()
    {
        return Ok(WasmContributionDecl::default());
    }
    let register = session
        .instance
        .get_typed_func::<(), u64>(&mut session.store, export::REGISTER)
        .map_err(|e| {
            pool::wasm_err(
                &format!(
                    "plugin '{}' export '{}' has an unexpected signature",
                    inner.manifest.id,
                    export::REGISTER
                ),
                e,
            )
        })?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let packed = with_call_timeout(inner, async {
        register
            .call_async(&mut session.store, ())
            .await
            .map_err(|e| guest_call_error(inner, export::REGISTER, e))
    })
    .await;
    let packed = packed?;
    let bytes = read_output(inner, &mut *session, packed).await?;
    abi::decode_decl(&bytes)
}

/// Call the `wf_dispatch` export. Required whenever the plugin declares
/// contributions. The session is checked out from the pool and returned
/// afterwards; only sessions from successful calls are eligible for reuse.
async fn invoke_dispatch(
    inner: &WasmPluginInner,
    handler_type: &str,
    handler_name: &str,
    input_json: &str,
) -> PluginResult<Vec<u8>> {
    let mut session = inner.pool.acquire().await?;
    let result =
        invoke_dispatch_inner(inner, &mut session, handler_type, handler_name, input_json).await;
    observe(inner, &session, &result);
    inner.pool.release(session, result.is_ok()).await;
    result
}

async fn invoke_dispatch_inner(
    inner: &WasmPluginInner,
    session: &mut PooledSession,
    handler_type: &str,
    handler_name: &str,
    input_json: &str,
) -> PluginResult<Vec<u8>> {
    let id = &inner.manifest.id;
    let dispatch = session
        .instance
        .get_typed_func::<(u32, u32, u32, u32, u32, u32), u64>(&mut session.store, export::DISPATCH)
        .map_err(|_| {
            PluginError::WasmError(format!(
                "plugin '{id}' declares contributions but does not export '{}'",
                export::DISPATCH
            ))
        })?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let type_ptr = write_input(inner, &mut *session, handler_type.as_bytes()).await?;
    let name_ptr = write_input(inner, &mut *session, handler_name.as_bytes()).await?;
    let input_ptr = write_input(inner, &mut *session, input_json.as_bytes()).await?;
    let packed = with_call_timeout(inner, async {
        dispatch
            .call_async(
                &mut session.store,
                (
                    type_ptr,
                    handler_type.len() as u32,
                    name_ptr,
                    handler_name.len() as u32,
                    input_ptr,
                    input_json.len() as u32,
                ),
            )
            .await
            .map_err(|e| guest_call_error(inner, export::DISPATCH, e))
    })
    .await;
    let packed = packed?;
    let out = read_output(inner, &mut *session, packed).await?;
    Ok(out)
}

fn hook_input(inner: &WasmPluginInner, config: &Value) -> PluginResult<Vec<u8>> {
    abi::encode_hook_input(&inner.manifest.id, config)
}

#[async_trait]
impl Plugin for WasmPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.inner.manifest
    }

    async fn on_load(&self, ctx: &PluginContext) -> PluginResult<()> {
        let input = hook_input(&self.inner, &ctx.config)?;
        invoke_hook(&self.inner, export::ON_LOAD, &input).await
    }

    async fn on_unload(&self, ctx: &PluginContext) -> PluginResult<()> {
        let input = hook_input(&self.inner, &ctx.config)?;
        invoke_hook(&self.inner, export::ON_UNLOAD, &input).await
    }

    async fn on_activate(&self, ctx: &PluginContext) -> PluginResult<()> {
        let input = hook_input(&self.inner, &ctx.config)?;
        invoke_hook(&self.inner, export::ON_ACTIVATE, &input).await
    }

    async fn on_deactivate(&self, ctx: &PluginContext) -> PluginResult<()> {
        let input = hook_input(&self.inner, &ctx.config)?;
        invoke_hook(&self.inner, export::ON_DEACTIVATE, &input).await
    }

    async fn on_config_change(&self, config: &Value) -> PluginResult<()> {
        let input = hook_input(&self.inner, config)?;
        invoke_hook(&self.inner, export::ON_CONFIG_CHANGE, &input).await
    }

    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        let decl = &self.inner.decl;
        for name in &decl.node_types {
            registrar.register_node_type(
                name,
                Arc::new(WasmNodeHandler {
                    inner: self.inner.clone(),
                    type_name: name.clone(),
                }),
            )?;
        }
        for name in &decl.tool_types {
            registrar.register_tool_type(
                name,
                Arc::new(WasmToolExecutor {
                    inner: self.inner.clone(),
                    type_name: name.clone(),
                }),
            )?;
        }
        for name in &decl.llm_providers {
            registrar.register_llm_provider(
                name,
                Arc::new(WasmLlmFormatter {
                    inner: self.inner.clone(),
                    name: name.clone(),
                }),
            )?;
        }
        for name in &decl.formatters {
            registrar.register_formatter(
                name,
                Arc::new(WasmLlmFormatter {
                    inner: self.inner.clone(),
                    name: name.clone(),
                }),
            )?;
        }
        for event_type in &decl.event_handlers {
            registrar.register_event_handler(
                event_type,
                Arc::new(WasmEventHandler {
                    inner: self.inner.clone(),
                    event_type: event_type.clone(),
                }),
            )?;
        }
        for mw in &decl.middleware {
            registrar.register_middleware(
                MiddlewarePhase::from(mw.phase.as_str()),
                mw.priority,
                Arc::new(WasmMiddlewareHandler {
                    inner: self.inner.clone(),
                    phase: mw.phase.clone(),
                }),
            )?;
        }
        Ok(())
    }
}

pub struct WasmNodeHandler {
    inner: Arc<WasmPluginInner>,
    type_name: String,
}

pub struct WasmToolExecutor {
    inner: Arc<WasmPluginInner>,
    type_name: String,
}

pub struct WasmLlmFormatter {
    inner: Arc<WasmPluginInner>,
    name: String,
}

pub struct WasmEventHandler {
    inner: Arc<WasmPluginInner>,
    event_type: String,
}

pub struct WasmMiddlewareHandler {
    inner: Arc<WasmPluginInner>,
    phase: String,
}

#[async_trait]
impl PluginNodeHandler for WasmNodeHandler {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult> {
        let input = serde_json::to_string(&ctx)
            .map_err(|e| PluginError::WasmError(format!("serialize node ctx: {e}")))?;
        let output = invoke_dispatch(&self.inner, "node", &self.type_name, &input).await?;
        serde_json::from_slice::<PluginNodeResult>(&output)
            .map_err(|e| PluginError::WasmError(format!("deserialize node result: {e}")))
    }
}

#[async_trait]
impl PluginToolExecutor for WasmToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let input = serde_json::to_string(&ctx)
            .map_err(|e| PluginError::WasmError(format!("serialize tool ctx: {e}")))?;
        let output = invoke_dispatch(&self.inner, "tool", &self.type_name, &input).await?;
        serde_json::from_slice::<PluginToolResult>(&output)
            .map_err(|e| PluginError::WasmError(format!("deserialize tool result: {e}")))
    }
}

#[async_trait]
impl PluginLlmFormatter for WasmLlmFormatter {
    async fn format(&self, request: PluginLlmRequest) -> PluginResult<PluginLlmResponse> {
        let input = serde_json::to_string(&request)
            .map_err(|e| PluginError::WasmError(format!("serialize llm request: {e}")))?;
        let output = invoke_dispatch(&self.inner, "llm", &self.name, &input).await?;
        serde_json::from_slice::<PluginLlmResponse>(&output)
            .map_err(|e| PluginError::WasmError(format!("deserialize llm response: {e}")))
    }
}

#[async_trait]
impl PluginEventHandler for WasmEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let input = serde_json::to_string(&event)
            .map_err(|e| PluginError::WasmError(format!("serialize event: {e}")))?;
        invoke_dispatch(&self.inner, "event", &self.event_type, &input).await?;
        Ok(())
    }
}

#[async_trait]
impl PluginMiddlewareHandler for WasmMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<()> {
        let input = serde_json::to_string(&context)
            .map_err(|e| PluginError::WasmError(format!("serialize middleware ctx: {e}")))?;
        let output = invoke_dispatch(&self.inner, "mw", &self.phase, &input).await?;
        let proceed: Value = serde_json::from_slice(&output).unwrap_or(Value::Null);
        if proceed.as_bool().unwrap_or(true) {
            next().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::policy::{resolve_limits, WasiGrants};
    use super::*;
    use crate::manifest::PluginType;

    #[tokio::test]
    async fn stats_count_calls_and_fuel() {
        let wat = super::super::loader::wasm_test_echo_wat(r#"{"tool_types":["stat_tool"]}"#);
        let bytes = wat::parse_str(&wat).expect("valid wat");
        let engine = pool::engine_handle();
        let module = pool::cached_module(&engine, &bytes).expect("compile");
        let linker = pool::new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let manifest = PluginManifest {
            id: "stat".into(),
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
            wasm: None,
        };
        let limits = resolve_limits(&manifest, 10_000).expect("limits");
        let engine_clone = engine.clone();
        let grants = WasiGrants::default();
        let stats = Arc::new(WasmStats::default());
        let pool = SessionPool::new(
            "stat",
            &engine_clone,
            &pre,
            &grants,
            &limits,
            &stats,
            module.get_export(export::HEAP_RESET).is_some(),
        );
        let mut inner = WasmPluginInner {
            manifest,
            engine,
            limits,
            decl: WasmContributionDecl::default(),
            stats,
            pool,
        };
        inner.decl = fetch_declaration(&inner).await.expect("decl");
        let plugin = WasmPlugin::from_inner(inner);

        let ctx = PluginContext {
            plugin_id: "stat".into(),
            sdk_version: "0.1.0".into(),
            config: Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: Arc::new(crate::contributions::ContributionManager::new()),
        };
        plugin.on_load(&ctx).await.expect("hook");
        let executor = PluginToolExecutorBridge(&plugin, "stat_tool");
        executor.call().await.expect("dispatch");

        let snap = plugin.stats();
        assert_eq!(snap.calls, 3);
        assert_eq!(snap.ok, 3);
        assert_eq!(snap.failed, 0);
        assert_eq!(snap.timeouts, 0);
        assert!(snap.fuel_consumed > 0, "metered calls consume fuel");
    }

    struct PluginToolExecutorBridge<'a>(&'a WasmPlugin, &'a str);

    impl PluginToolExecutorBridge<'_> {
        async fn call(&self) -> PluginResult<PluginToolResult> {
            let handler = WasmToolExecutor {
                inner: self.0.inner.clone(),
                type_name: self.1.to_owned(),
            };
            handler
                .execute(PluginToolContext {
                    args: serde_json::json!({}),
                })
                .await
        }
    }

    /// Build a `WasmPlugin` directly from WAT plus a pool size, mirroring
    /// the loader path (probe reset export, build pool, fetch declaration).
    async fn build_pooled_plugin(id: &str, wat: &str, pool_size: u32) -> WasmPlugin {
        use wf_plugin_sdk::manifest::WasmConfig;

        let bytes = wat::parse_str(wat).expect("valid wat");
        let engine = pool::engine_handle();
        let module = pool::cached_module(&engine, &bytes).expect("compile");
        let linker = pool::new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let manifest = PluginManifest {
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
            wasm: Some(WasmConfig {
                store_pool_size: Some(pool_size),
                ..Default::default()
            }),
        };
        let limits = resolve_limits(&manifest, 10_000).expect("limits");
        let grants = WasiGrants::default();
        let stats = Arc::new(WasmStats::default());
        let pool = SessionPool::new(
            id,
            &engine,
            &pre,
            &grants,
            &limits,
            &stats,
            module.get_export(export::HEAP_RESET).is_some(),
        );
        let mut inner = WasmPluginInner {
            manifest,
            engine,
            limits,
            decl: WasmContributionDecl::default(),
            stats,
            pool,
        };
        inner.decl = fetch_declaration(&inner).await.expect("decl");
        WasmPlugin::from_inner(inner)
    }

    fn hook_context(plugin_id: &str) -> PluginContext {
        PluginContext {
            plugin_id: plugin_id.to_owned(),
            sdk_version: "0.1.0".into(),
            config: Value::Null,
            logger: crate::context::PluginLogger,
            contribution_manager: Arc::new(crate::contributions::ContributionManager::new()),
        }
    }

    #[tokio::test]
    async fn pool_reuses_session_after_successful_reset() {
        use super::super::loader::wasm_test_echo_reset_wat;

        let wat = wasm_test_echo_reset_wat(r#"{"tool_types":["pooled_tool"]}"#, 0);
        let plugin = build_pooled_plugin("pooled", &wat, 2).await;
        assert!(plugin.inner.pool.enabled());
        // The load-time register call already returned its session.
        assert_eq!(plugin.stats().pool_misses, 1);

        let ctx = hook_context("pooled");
        plugin.on_load(&ctx).await.expect("first hook");
        assert_eq!(plugin.stats().pool_hits, 1);
        PluginToolExecutorBridge(&plugin, "pooled_tool")
            .call()
            .await
            .expect("dispatch on reused session");
        let snap = plugin.stats();
        assert_eq!(snap.pool_hits, 2);
        assert_eq!(snap.pool_misses, 1);
        assert_eq!(snap.pool_drops, 0);
        assert_eq!(snap.calls, 3);
        assert_eq!(snap.ok, 3);
    }

    #[tokio::test]
    async fn pool_stays_disabled_without_reset_export() {
        use super::super::loader::wasm_test_echo_wat;

        let wat = wasm_test_echo_wat(r#"{"tool_types":[]}"#);
        let plugin = build_pooled_plugin("noreset", &wat, 2).await;
        assert!(!plugin.inner.pool.enabled());

        let ctx = hook_context("noreset");
        plugin.on_load(&ctx).await.expect("hook");
        plugin.on_load(&ctx).await.expect("hook");
        let snap = plugin.stats();
        assert_eq!(snap.pool_hits, 0);
        assert_eq!(snap.pool_misses, 0);
        assert_eq!(snap.pool_drops, 0);
        assert_eq!(snap.calls, 3);
    }

    #[tokio::test]
    async fn refused_reset_discards_session() {
        use super::super::loader::wasm_test_echo_reset_wat;

        let wat = wasm_test_echo_reset_wat(r#"{"tool_types":[]}"#, 1);
        let plugin = build_pooled_plugin("refuse", &wat, 2).await;
        assert!(plugin.inner.pool.enabled());
        // Load-time register already refused once.
        assert_eq!(plugin.stats().pool_drops, 1);

        plugin
            .on_load(&hook_context("refuse"))
            .await
            .expect("hook still succeeds");
        let snap = plugin.stats();
        assert_eq!(snap.pool_hits, 0);
        assert_eq!(snap.pool_drops, 2);
        assert_eq!(snap.ok, snap.calls);
    }

    #[tokio::test]
    async fn failed_call_discards_session() {
        let wat = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "{\"tool_types\":[\"t\"]}")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_heap_reset") (result i32)
    (global.set $heap (i32.const 4096))
    (i32.const 0))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 20)) (i64.const 32))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (unreachable)))
"#;
        let plugin = build_pooled_plugin("trap", wat, 2).await;
        // Register succeeded, so one session was pooled at load.
        assert_eq!(plugin.stats().pool_drops, 0);

        let err = PluginToolExecutorBridge(&plugin, "t")
            .call()
            .await
            .expect_err("trapping dispatch must fail");
        assert!(
            matches!(err, PluginError::WasmError(_)),
            "expected wasm error, got {err:?}"
        );
        let snap = plugin.stats();
        assert_eq!(
            snap.pool_hits, 1,
            "failed call checked out the idle session"
        );
        assert_eq!(snap.pool_drops, 1, "post-trap session is discarded");
        assert_eq!(snap.failed, 1);
    }
}
