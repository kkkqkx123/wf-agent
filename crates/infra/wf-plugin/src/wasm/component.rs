//! Component-model guest host (`wf:plugin@1.0.0` world).
//!
//! Companion to the core-module path in `plugin.rs`: component
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

use super::policy::{WasiGrants, WasmLimits};
use super::stats::WasmStats;
use super::stdio::GuestLogPipe;
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

/// Host implementation of the `wf:plugin/host-log` world import: structured
/// guest-to-host logging. Best-effort; over-long messages are truncated by
/// the shared emitter and the call never fails the guest.
impl gen::wf::plugin::host_log::Host for ComponentHostState {
    async fn log(&mut self, level: u32, message: String) {
        let plugin_id = self.plugin_id.clone();
        super::stdio::emit_host_log(&plugin_id, level, message.as_bytes());
    }
}

mod codec;
mod handlers;
mod observe;
mod runtime;
mod session_pool;
mod version;

pub(crate) use codec::*;
pub(crate) use handlers::*;
pub(crate) use observe::*;
pub(crate) use runtime::*;
pub(crate) use session_pool::*;
pub(crate) use version::*;

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

#[cfg(test)]
#[path = "component/tests.rs"]
mod tests;
