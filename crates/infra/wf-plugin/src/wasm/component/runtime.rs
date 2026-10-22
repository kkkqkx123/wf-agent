use super::GeneratedPlugin;
use super::HookInput;
use super::*;

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
    pub(crate) inner: Arc<ComponentPluginInner>,
}

impl ComponentPlugin {
    pub(crate) fn from_inner(inner: ComponentPluginInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn stats(&self) -> super::super::stats::WasmStatsSnapshot {
        self.inner.stats.snapshot()
    }
}

/// Marker connecting the generated world bindings to [`ComponentHostState`].
struct ComponentHostView;

impl wasmtime::component::HasData for ComponentHostView {
    type Data<'a> = &'a mut ComponentHostState;
}

/// Build the linker shared by every component plugin: WASI p2 imports plus
/// the `wf:plugin/host-log` guest-to-host namespace.
pub(crate) fn new_component_linker(engine: &Engine) -> PluginResult<Linker<ComponentHostState>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| super::super::pool::wasm_err("component wasi linker setup failed", e))?;
    GeneratedPlugin::add_to_linker::<ComponentHostState, ComponentHostView>(&mut linker, |state| {
        state
    })
    .map_err(|e| super::super::pool::wasm_err("component host-log linker setup failed", e))?;
    Ok(linker)
}

/// Build one component session: WASI context from grants, fuel budget and
/// memory cap from limits. Runs on the caller's thread; component entry
/// points invoke it inside `spawn_blocking`.
pub(crate) fn build_component_store(
    engine: &Engine,
    plugin_id: &str,
    grants: &WasiGrants,
    limits: &WasmLimits,
) -> PluginResult<Store<ComponentHostState>> {
    let stdout = super::super::stdio::GuestLogPipe::new(super::super::stdio::GUEST_STDIO_CAP_BYTES);
    let stderr = super::super::stdio::GuestLogPipe::new(super::super::stdio::GUEST_STDIO_CAP_BYTES);
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
        .map_err(|e| super::super::pool::wasm_err("component fuel setup failed", e))?;
    // Guest `_initialize` runs during instantiation; park the epoch deadline
    // so a language runtime's startup loops are not interrupted. Calls re-arm
    // a finite deadline through `arm_epoch`.
    store.set_epoch_deadline(super::super::pool::INITIALIZE_EPOCH_DEADLINE_TICKS);
    Ok(store)
}

/// Run one component operation with the outer timeout backstop. The epoch
/// deadline (armed on the store before the call) fires first for spinning
/// guests; this only guards host-side stalls. Every outcome is recorded in
/// stats with its real error value so timeouts and failures stay distinct.
pub(crate) async fn guard_component_call<T>(
    plugin_id: &str,
    limits: &WasmLimits,
    stats: &WasmStats,
    call: impl std::future::Future<Output = PluginResult<T>>,
) -> PluginResult<T> {
    let outcome = match super::super::pool::outer_timeout_ms(limits.call_timeout_ms) {
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

/// Call one lifecycle hook. The session is checked out from the pool and
/// returned afterwards; only sessions from successful calls are eligible
/// for reuse. A component instantiates only when it implements the full
/// world, so unlike the tolerant core-module path a bind failure here
/// means host/guest skew and fails loudly.
pub(crate) async fn invoke_component_hook(
    inner: &Arc<ComponentPluginInner>,
    op: HookOp,
    input: Option<HookInput>,
) -> PluginResult<()> {
    let name = op.name();
    let mut session = inner.pool.acquire().await?;
    super::super::pool::arm_epoch(
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
    super::super::pool::arm_epoch(
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
            .map_err(|e| super::super::pool::wasm_err("register call failed", e))
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
    super::super::policy::validate_network_policy(manifest)?;
    tracing::debug!(
        "wasm component '{id}' loading against world version {WF_COMPONENT_WORLD_VERSION}"
    );
    check_component_world_version(&id, bytes)?;
    if bytes.len() as u64 > limits.max_module_bytes {
        return Err(PluginError::LoadFailed(format!(
            "wasm component for plugin '{id}' ({} bytes) exceeds limit of {} bytes",
            bytes.len(),
            limits.max_module_bytes
        )));
    }
    let engine = super::super::pool::engine_handle();
    let component = cached_component(&engine, bytes).map_err(|e| match e {
        PluginError::WasmError(text) => {
            PluginError::WasmError(format!("plugin '{id}' component compile failed: {text}"))
        }
        other => other,
    })?;
    let linker = new_component_linker(&engine)?;
    let pre = linker.instantiate_pre(&component).map_err(|e| {
        super::super::pool::wasm_err(&format!("plugin '{id}' pre-instantiation failed"), e)
    })?;
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

pub(crate) fn component_cache() -> &'static moka::sync::Cache<String, Component> {
    static CACHE: std::sync::OnceLock<moka::sync::Cache<String, Component>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(super::super::pool::MAX_CACHED_ARTIFACTS)
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
    let component = Component::new(engine, bytes)
        .map_err(|e| super::super::pool::wasm_err("component compile failed", e))?;
    component_cache().insert(digest, component.clone());
    Ok(component)
}

/// Call the component `dispatch` export. Required whenever the plugin
/// declares contributions; a guest `Err` becomes a plugin error. The
/// session is checked out from the pool and returned afterwards; only
/// sessions from successful calls are eligible for reuse.
pub(crate) async fn invoke_component_dispatch(
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
    super::super::pool::arm_epoch(
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

pub(crate) fn config_json(config: &Value) -> PluginResult<String> {
    serde_json::to_string(config)
        .map_err(|e| PluginError::WasmError(format!("hook input serialize failed: {e}")))
}
