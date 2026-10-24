use super::*;
use async_trait::async_trait;

pub(crate) struct WasmPluginInner {
    pub(crate) manifest: PluginManifest,
    pub(crate) engine: Engine,
    pub(crate) limits: WasmLimits,
    pub(crate) decl: RwLock<WasmContributionDecl>,
    pub(crate) stats: Arc<WasmStats>,
    pub(crate) pool: SessionPool,
}

pub struct WasmPlugin {
    pub(crate) inner: Arc<WasmPluginInner>,
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

    pub fn stats(&self) -> super::super::stats::WasmStatsSnapshot {
        self.inner.stats.snapshot()
    }
}

pub(crate) async fn fetch_declaration(
    inner: &WasmPluginInner,
) -> PluginResult<WasmContributionDecl> {
    let mut session = inner.pool.acquire().await?;
    let negotiated = match abi::negotiate_abi_version(
        &mut session,
        &inner.engine,
        &inner.manifest.id,
        &inner.limits,
    )
    .await
    {
        Ok(version) => version,
        Err(e) => {
            let result: PluginResult<WasmContributionDecl> = Err(e);
            drain_stdio(inner, export::SUPPORTED_ABI_VERSIONS, &session);
            observe(inner, &session, &result);
            inner.pool.release(session, false).await;
            return result;
        }
    };
    if let Some(expected) = abi::selected_entry_version(&inner.manifest) {
        if expected != negotiated {
            let result: PluginResult<WasmContributionDecl> = Err(PluginError::LoadFailed(format!(
                "plugin '{}' manifest selects entry for abi version {expected} but the guest negotiated version {negotiated}. Check versioned_entry_points keys or the artifact placed at the selected path.",
                inner.manifest.id
            )));
            drain_stdio(inner, export::SUPPORTED_ABI_VERSIONS, &session);
            observe(inner, &session, &result);
            inner.pool.release(session, false).await;
            return result;
        }
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

    async fn reload_declaration(&self) -> PluginResult<bool> {
        let fresh = fetch_declaration(&self.inner).await?;
        let mut current = wf_common::lock::write_ok(self.inner.decl.write());
        if *current == fresh {
            return Ok(false);
        }
        *current = fresh;
        Ok(true)
    }

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
        for name in &decl.llm_auths {
            registrar.register_llm_auth(
                name,
                Arc::new(WasmLlmAuth {
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
        super::super::warn_unknown_middleware_phases(&self.inner.manifest.id, &decl.middleware);
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
        super::super::shared::register_decl_tools(&decl.tools, &decl.tool_descriptions, registrar)?;
        crate::manifest::register_manifest_tools(&self.inner.manifest, registrar)?;
        Ok(())
    }
}
