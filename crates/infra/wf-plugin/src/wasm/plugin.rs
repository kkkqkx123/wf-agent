use std::sync::{Arc, RwLock};
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
    /// Cached contribution declaration. Locked because `reload_declaration`
    /// may replace it while registered handlers hold the same inner.
    pub(crate) decl: RwLock<WasmContributionDecl>,
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

    pub fn declaration(&self) -> WasmContributionDecl {
        wf_common::lock::read_ok(self.inner.decl.read()).clone()
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

/// Drain captured guest `stdout`/`stderr` to the host log. Runs after every
/// guest call (success or failure) so pooled sessions never carry output
/// across calls and failures stay diagnosable.
fn drain_stdio(inner: &WasmPluginInner, op: &str, session: &PooledSession) {
    let state = session.store.data();
    super::stdio::drain_guest_stdio(&inner.manifest.id, op, &state.stdout, &state.stderr);
}

/// Call a lifecycle hook export with a JSON envelope. A missing export
/// counts as success, matching the Lua loader convention. The session is
/// checked out from the pool and returned afterwards; only sessions from
/// successful calls are eligible for reuse.
async fn invoke_hook(inner: &WasmPluginInner, export_name: &str, input: &[u8]) -> PluginResult<()> {
    let mut session = inner.pool.acquire().await?;
    let result = invoke_hook_inner(inner, &mut session, export_name, input).await;
    drain_stdio(inner, export_name, &session);
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
    if code == 0 {
        return Ok(());
    }
    let detail = read_last_error(inner, &mut *session).await;
    abi::check_hook_status_with_detail(export_name, &inner.manifest.id, code, detail.as_deref())
}

/// Read the optional guest `wf_last_error` detail string after a hook
/// reported failure. Best-effort: any lookup/call/read problem yields
/// `None` so error-detail probing never masks the original status code.
async fn read_last_error(inner: &WasmPluginInner, session: &mut PooledSession) -> Option<String> {
    session
        .instance
        .get_func(&mut session.store, export::LAST_ERROR)?;
    let detail_fn = session
        .instance
        .get_typed_func::<(), u64>(&mut session.store, export::LAST_ERROR)
        .ok()?;
    pool::arm_epoch(
        &inner.engine,
        &mut session.store,
        inner.limits.call_timeout_ms,
    );
    let packed = with_call_timeout(inner, async {
        detail_fn
            .call_async(&mut session.store, ())
            .await
            .map_err(|e| guest_call_error(inner, export::LAST_ERROR, e))
    })
    .await
    .ok()?;
    let (ptr, len) = abi::unpack_ptr_len(packed);
    let bytes = abi::read_bytes(&session.store, &session.memory, ptr, len).ok()?;
    release_output(inner, session, ptr, len).await;
    let mut text = String::from_utf8_lossy(&bytes).trim().to_owned();
    if text.is_empty() {
        return None;
    }
    if text.len() > 512 {
        text.truncate(512);
        text.push_str("...(truncated)");
    }
    Some(text)
}

/// Fetch the contribution declaration by calling `wf_register`.
/// Used once at load time; the result is cached on the plugin.
/// The core-module ABI version is negotiated first so an upgraded
/// guest never silently misbehaves.
pub(crate) async fn fetch_declaration(
    inner: &WasmPluginInner,
) -> PluginResult<WasmContributionDecl> {
    let mut session = inner.pool.acquire().await?;
    if let Err(e) = abi::negotiate_abi_version(
        &mut session,
        &inner.engine,
        &inner.manifest.id,
        &inner.limits,
    )
    .await
    {
        let result: PluginResult<WasmContributionDecl> = Err(e);
        drain_stdio(inner, export::ABI_VERSION, &session);
        observe(inner, &session, &result);
        inner.pool.release(session, false).await;
        return result;
    }
    let result = fetch_declaration_inner(inner, &mut session).await;
    drain_stdio(inner, export::REGISTER, &session);
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
    let decl = abi::decode_decl(&bytes)?;
    Ok(decl)
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
    drain_stdio(inner, export::DISPATCH, &session);
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

    /// Re-call `wf_register` and adopt the result when it differs from the
    /// cached declaration. Returns true exactly when the engine must
    /// re-sync contributions.
    async fn reload_declaration(&self) -> PluginResult<bool> {
        let fresh = fetch_declaration(&self.inner).await?;
        let mut current = wf_common::lock::write_ok(self.inner.decl.write());
        if *current == fresh {
            return Ok(false);
        }
        *current = fresh;
        Ok(true)
    }

    /// The six registration loops intentionally mirror the component-model
    /// path: unifying them would require handler types generic over both
    /// backends for no behavioral gain. Shared call logic lives in
    /// `super::shared` instead.
    fn register_contributions(
        &self,
        registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        let decl = wf_common::lock::read_ok(self.inner.decl.read());
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
                Arc::new(WasmLlmCodec {
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
        super::warn_unknown_middleware_phases(&self.inner.manifest.id, &decl.middleware);
        for mw in &decl.middleware {
            let phase = MiddlewarePhase::from(mw.phase.as_str());
            registrar.register_middleware(
                phase,
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

pub struct WasmLlmCodec {
    inner: Arc<WasmPluginInner>,
    name: String,
}

impl WasmLlmCodec {
    /// Structured codec round-trip over the guest dispatch channel.
    /// The sync `PluginLlmCodec` contract cannot await, so block the
    /// current thread on the async dispatch; inside a tokio runtime this
    /// uses `block_in_place` to keep async workers free.
    fn roundtrip<T: serde::de::DeserializeOwned>(&self, op: &str, input: Value) -> PluginResult<T> {
        let input_json = super::shared::encode_call_input(&input, "llm codec input")?;
        let inner = self.inner.clone();
        let handler = format!("{}/{}", self.name, op);
        let output = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| {
                handle.block_on(invoke_dispatch(&inner, "llm-codec", &handler, &input_json))
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

impl PluginLlmCodec for WasmLlmCodec {
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
        let input = super::shared::encode_call_input(&ctx, "node ctx")?;
        let output = invoke_dispatch(&self.inner, "node", &self.type_name, &input).await?;
        super::shared::decode_call_output(&output, "node result")
    }
}

#[async_trait]
impl PluginToolExecutor for WasmToolExecutor {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult> {
        let input = super::shared::encode_call_input(&ctx, "tool ctx")?;
        let output = invoke_dispatch(&self.inner, "tool", &self.type_name, &input).await?;
        super::shared::decode_call_output(&output, "tool result")
    }
}

#[async_trait]
impl PluginEventHandler for WasmEventHandler {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()> {
        let input = super::shared::encode_call_input(&event, "event")?;
        invoke_dispatch(&self.inner, "event", &self.event_type, &input).await?;
        Ok(())
    }
}

#[async_trait]
impl PluginMiddlewareHandler for WasmMiddlewareHandler {
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value> {
        let input = super::shared::encode_call_input(&context, "middleware ctx")?;
        let output = invoke_dispatch(&self.inner, "mw", &self.phase, &input).await?;
        super::shared::resolve_middleware_output(&output, &context, next).await
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
            llm_providers: vec![],
            wasm: None,
            lua: None,
        };
        let limits = resolve_limits(&manifest, 10_000);
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
        let inner = WasmPluginInner {
            manifest,
            engine,
            limits,
            decl: RwLock::new(Default::default()),
            stats,
            pool,
        };
        let decl = fetch_declaration(&inner).await.expect("decl");
        *wf_common::lock::write_ok(inner.decl.write()) = decl;
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
            llm_providers: vec![],
            wasm: Some(WasmConfig {
                store_pool_size: Some(pool_size),
                ..Default::default()
            }),
            lua: None,
        };
        let limits = resolve_limits(&manifest, 10_000);
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
        let inner = WasmPluginInner {
            manifest,
            engine,
            limits,
            decl: RwLock::new(Default::default()),
            stats,
            pool,
        };
        let decl = fetch_declaration(&inner).await.expect("decl");
        *wf_common::lock::write_ok(inner.decl.write()) = decl;
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

    const ABI_PROBE_BASE_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (func (export "alloc") (param $n i32) (result i32) (i32.const 0)))
"#;

    /// Build one live session from WAT and run the ABI version probe.
    async fn negotiate_probe(wat: &str) -> PluginResult<()> {
        let bytes = wat::parse_str(wat).expect("valid wat");
        let engine = pool::engine_handle();
        let module = pool::cached_module(&engine, &bytes).expect("compile");
        let linker = pool::new_linker(&engine).expect("linker");
        let pre = linker.instantiate_pre(&module).expect("pre-instantiate");
        let manifest = PluginManifest {
            id: "abi-probe".into(),
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
            wasm: None,
            lua: None,
        };
        let limits = resolve_limits(&manifest, 10_000);
        let grants = WasiGrants::default();
        let mut session = pool::new_session(&engine, &pre, "abi-probe", &grants, &limits).await?;
        super::abi::negotiate_abi_version(&mut session, &engine, "abi-probe", &limits).await
    }

    fn with_version_func(base: &str, func: &str) -> String {
        let stripped = base.trim_end().strip_suffix(')').expect("wat ends");
        format!("{stripped}\n  {func}\n)\n")
    }

    #[tokio::test]
    async fn abi_version_missing_means_v1() {
        negotiate_probe(ABI_PROBE_BASE_WAT)
            .await
            .expect("absent version is compatible");
    }

    #[tokio::test]
    async fn abi_version_match_accepts_v1() {
        let wat = with_version_func(
            ABI_PROBE_BASE_WAT,
            r#"(func (export "wf_abi_version") (result i32) (i32.const 1))"#,
        );
        negotiate_probe(&wat).await.expect("v1 is compatible");
    }

    #[tokio::test]
    async fn abi_version_mismatch_fails_load() {
        let wat = with_version_func(
            ABI_PROBE_BASE_WAT,
            r#"(func (export "wf_abi_version") (result i32) (i32.const 99))"#,
        );
        let err = negotiate_probe(&wat).await.expect_err("v99 must fail");
        assert!(err.to_string().contains("incompatible abi"), "got: {err}");
    }

    #[tokio::test]
    async fn abi_version_wrong_signature_fails_load() {
        let wat = with_version_func(
            ABI_PROBE_BASE_WAT,
            r#"(func (export "wf_abi_version") (param i32) (result i32) (local.get 0))"#,
        );
        let err = negotiate_probe(&wat)
            .await
            .expect_err("wrong signature must fail");
        assert!(
            err.to_string().contains("unexpected signature"),
            "got: {err}"
        );
    }

    const HOOK_FAIL_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "bad config")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32) (i32.const 7))
  (func (export "wf_last_error") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 10)) (i64.const 32)))))
"#;

    const HOOK_FAIL_NO_DETAIL_WAT: &str = r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32) (i32.const 7)))
"#;

    #[tokio::test]
    async fn hook_failure_carries_last_error_detail() {
        let plugin = build_pooled_plugin("detail", HOOK_FAIL_WAT, 0).await;
        let err = plugin
            .on_load(&hook_context("detail"))
            .await
            .expect_err("hook code 7 must fail");
        let text = err.to_string();
        assert!(text.contains("detail"), "got: {text}");
        assert!(text.contains("code 7"), "got: {text}");
        assert!(text.contains("bad config"), "got: {text}");
    }

    #[tokio::test]
    async fn hook_failure_without_last_error_has_code_only() {
        let plugin = build_pooled_plugin("nocode-detail", HOOK_FAIL_NO_DETAIL_WAT, 0).await;
        let err = plugin
            .on_load(&hook_context("nocode-detail"))
            .await
            .expect_err("hook code 7 must fail");
        let text = err.to_string();
        assert!(text.contains("code 7"), "got: {text}");
    }

    /// Guest whose `wf_dispatch` always answers with `response_json`.
    /// Used to drive middleware envelope cases without a real chain.
    fn mw_response_wat(response_json: &str) -> String {
        let escaped = response_json.replace('\\', "\\\\").replace('"', "\\\"");
        let len = response_json.len();
        format!(
            r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const 0) "{{}}")
  (data (i32.const 1024) "{escaped}")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 2)) (i64.const 32))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (i64.or (i64.extend_i32_u (i32.const 1024)) (i64.shl (i64.extend_i32_u (i32.const {len})) (i64.const 32)))))
"#
        )
    }

    async fn drive_middleware(
        id: &str,
        response_json: &str,
        input: Value,
        next: NextFn,
    ) -> PluginResult<Value> {
        let plugin = build_pooled_plugin(id, &mw_response_wat(response_json), 0).await;
        let handler = WasmMiddlewareHandler {
            inner: plugin.inner.clone(),
            phase: "pre_tool".to_owned(),
        };
        handler.handle(input, next).await
    }

    #[tokio::test]
    async fn middleware_envelope_rewrites_downstream_context() {
        use std::sync::Mutex;

        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_next = seen.clone();
        let next: NextFn = Box::new(move |ctx| {
            seen_next.lock().expect("lock").push(ctx.clone());
            Box::pin(async move { Ok(ctx) })
        });
        let out = drive_middleware(
            "mw-rewrite",
            r#"{"proceed":true,"context":{"patched":1}}"#,
            serde_json::json!({"a": 1}),
            next,
        )
        .await
        .expect("middleware runs");
        assert_eq!(out, serde_json::json!({"patched": 1}));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![serde_json::json!({"patched": 1})]
        );
    }

    #[tokio::test]
    async fn middleware_envelope_short_circuits_chain() {
        let next: NextFn = Box::new(|_| {
            Box::pin(async move {
                panic!("short-circuited chain must not call next");
            })
        });
        let out = drive_middleware(
            "mw-stop",
            r#"{"proceed":false,"context":{"stopped":true}}"#,
            serde_json::json!({"a": 1}),
            next,
        )
        .await
        .expect("middleware runs");
        assert_eq!(out, serde_json::json!({"stopped": true}));
    }

    #[tokio::test]
    async fn middleware_legacy_bool_still_proceeds() {
        let next: NextFn = Box::new(|ctx| Box::pin(async move { Ok(ctx) }));
        let out = drive_middleware("mw-legacy", "true", serde_json::json!({"a": 1}), next)
            .await
            .expect("middleware runs");
        assert_eq!(out, serde_json::json!({"a": 1}));
    }

    /// Guest whose declaration grows after its `toggle` tool runs: a
    /// mutable flag selects between two static declaration documents.
    /// Requires a pooled session (plus the heap-reset hook): without reuse
    /// every call would see a fresh flag and the change would never stick.
    fn redeclaring_guest_wat() -> String {
        let before = r#"{"tool_types":["toggle"]}"#;
        let after = r#"{"tool_types":["toggle","extra"]}"#;
        let result = r#"{"result":{}}"#;
        let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let (before_escaped, after_escaped, result_escaped) =
            (escape(before), escape(after), escape(result));
        format!(
            r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (global $flipped (mut i32) (i32.const 0))
  (data (i32.const 0) "{before_escaped}")
  (data (i32.const 128) "{after_escaped}")
  (data (i32.const 256) "{result_escaped}")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_heap_reset") (result i32)
    (global.set $heap (i32.const 4096))
    (i32.const 0))
  (func (export "wf_register") (result i64)
    (if (result i64) (i32.eqz (global.get $flipped))
      (then (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const {before_len})) (i64.const 32))))
      (else (i64.or (i64.extend_i32_u (i32.const 128)) (i64.shl (i64.extend_i32_u (i32.const {after_len})) (i64.const 32))))))
  (func (export "wf_dispatch")
    (param $tp i32) (param $tl i32) (param $np i32) (param $nl i32)
    (param $ip i32) (param $il i32) (result i64)
    (global.set $flipped (i32.const 1))
    (i64.or (i64.extend_i32_u (i32.const 256)) (i64.shl (i64.extend_i32_u (i32.const {result_len})) (i64.const 32)))))
"#,
            before_len = before.len(),
            after_len = after.len(),
            result_len = result.len(),
        )
    }

    #[tokio::test]
    async fn reload_reports_unchanged_for_static_guest() {
        use super::super::loader::wasm_test_echo_wat;

        let wat = wasm_test_echo_wat(r#"{"tool_types":[]}"#);
        let plugin = build_pooled_plugin("static-decl", &wat, 2).await;
        assert!(!plugin.reload_declaration().await.expect("reload"));
        assert!(plugin.declaration().tool_types.is_empty());
    }

    #[tokio::test]
    async fn reload_adopts_grown_declaration() {
        let plugin = build_pooled_plugin("redecl", &redeclaring_guest_wat(), 2).await;
        assert!(plugin.inner.pool.enabled());
        assert_eq!(plugin.declaration().tool_types, vec!["toggle".to_owned()]);
        assert!(!plugin.reload_declaration().await.expect("reload"));

        PluginToolExecutorBridge(&plugin, "toggle")
            .call()
            .await
            .expect("toggle flips the guest flag");
        assert!(plugin.reload_declaration().await.expect("reload"));
        assert_eq!(
            plugin.declaration().tool_types,
            vec!["toggle".to_owned(), "extra".to_owned()]
        );

        // The newly declared contribution registers and runs.
        let manager = crate::contributions::ContributionManager::new();
        manager.start_registration("redecl");
        {
            let mut registrar = manager.as_registrar();
            plugin
                .register_contributions(&mut registrar)
                .expect("contributions register");
        }
        manager
            .get_tool_executor("extra")
            .expect("extra tool registered");
    }

    /// Guest calling the `wf_host::log` import from `wf_on_load`, then
    /// succeeding. Proves the host namespace resolves and the call traps
    /// nothing.
    const HOST_LOG_WAT: &str = r#"(module
  (import "wf_host" "log" (func $hlog (param i32 i32 i32)))
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 128))
  (data (i32.const 0) "{}")
  (data (i32.const 64) "hello-host")
  (func (export "alloc") (param $n i32) (result i32)
    (local $p i32) (global.get $heap) (local.set $p)
    (global.set $heap (i32.add (global.get $heap) (local.get $n)))
    (local.get $p))
  (func (export "wf_on_load") (param $p i32) (param $n i32) (result i32)
    (call $hlog (i32.const 2) (i32.const 64) (i32.const 10))
    (i32.const 0))
  (func (export "wf_register") (result i64)
    (i64.or (i64.extend_i32_u (i32.const 0)) (i64.shl (i64.extend_i32_u (i32.const 2)) (i64.const 32)))))
"#;

    #[tokio::test]
    async fn guest_host_log_import_resolves_and_runs() {
        let dir = std::env::temp_dir().join("wf-wasm-test-hostlog");
        let _ = std::fs::create_dir_all(&dir);
        let bytes = wat::parse_str(HOST_LOG_WAT).expect("valid wat");
        std::fs::write(dir.join("plugin.wasm"), &bytes).expect("write module");
        let manifest = PluginManifest {
            id: "hostlog".into(),
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
            wasm: None,
            lua: None,
        };
        let plugin = super::super::loader::load_wasm_plugin_with_base(&manifest, &dir)
            .await
            .expect("load with host-log import");
        plugin
            .on_load(&hook_context("hostlog"))
            .await
            .expect("host log call succeeds");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
