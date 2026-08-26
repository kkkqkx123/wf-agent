//! Adapter turning a `wf_resource::resource_plugin::ResourcePlugin` (the
//! declarative "resource plugin") into a `wf_plugin::Plugin` so
//! built-in resource plugins activate through the unified plugin engine.
//!
//! The adapter lives in `wf-runtime` (not `wf-resource`) on purpose:
//! `wf-resource` must stay free of a `wf-plugin` dependency, so the glue
//! between the two plugin systems lives where both crates are visible.
//!
//! Lifecycle mapping:
//! - `register_contributions` → `on_before_assemble` → `assemble(config)` →
//!   register the bundle items as declarative contributions →
//!   `on_after_install(bundle)`. The contribution bridge then lands them in
//!   `ResourceRegistries` / `ToolRegistry` on activation.
//! - `on_deactivate` → `on_after_uninstall` (the bridge has already removed
//!   the plugin's resources by then). `on_before_uninstall` has no clean
//!   call site in the engine flow and is intentionally not invoked.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use wf_plugin::{ContributionRegistrar, Plugin, PluginContext, PluginManifest, PluginResult};
use wf_resource::predefined::resource_plugin::builtin_resource_plugins;
use wf_resource::resource_plugin::ResourcePlugin;

use crate::error::RuntimeResult;

/// Built-in resource plugins that are always available to the engine.
pub struct ResourcePluginAdapter {
    manifest: PluginManifest,
    inner: Arc<dyn ResourcePlugin>,
    config: Value,
}

impl ResourcePluginAdapter {
    pub fn new(inner: Arc<dyn ResourcePlugin>, config: Value) -> Self {
        let meta = inner.metadata();
        let manifest = PluginManifest {
            id: meta.id.clone(),
            version: meta.version,
            name: Some(meta.name),
            description: Some(meta.description),
            plugin_type: None,
            sdk_version: None,
            entry_point: format!("builtin://resource-plugin/{}", meta.id),
            dependencies: meta
                .dependencies
                .unwrap_or_default()
                .into_iter()
                .map(|d| (d, "*".to_string()))
                .collect(),
            optional_dependencies: Default::default(),
            contributions: vec![
                "workflow".into(),
                "prompt".into(),
                "agent-template".into(),
                "node-template".into(),
                "trigger".into(),
                "tool-description".into(),
                "tool".into(),
            ],
            config: Some(config.clone()),
            hooks: None,
        };
        Self {
            manifest,
            inner,
            config,
        }
    }
}

#[async_trait]
impl Plugin for ResourcePluginAdapter {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn register_contributions(&self, registrar: &mut dyn ContributionRegistrar) {
        if let Err(e) = self.inner.on_before_assemble(&self.config) {
            tracing::error!(
                "resource plugin '{}' on_before_assemble failed: {}",
                self.manifest.id,
                e
            );
            return;
        }
        let bundle = match self.inner.assemble(&self.config) {
            Ok(bundle) => bundle,
            Err(e) => {
                tracing::error!(
                    "resource plugin '{}' assemble failed: {}",
                    self.manifest.id,
                    e
                );
                return;
            }
        };

        for wf in &bundle.workflows {
            registrar.register_workflow(&wf.id, wf.clone());
        }
        for t in &bundle.prompts {
            registrar.register_prompt(&t.id, t.clone());
        }
        for a in &bundle.agent_templates {
            registrar.register_agent_template(&a.id, a.clone());
        }
        for n in &bundle.node_templates {
            registrar.register_node_template(&n.id, n.clone());
        }
        for t in &bundle.triggers {
            registrar.register_trigger(&t.name, t.clone());
        }
        for tool in &bundle.tools {
            registrar.register_tool(&tool.id, tool.clone());
        }

        if let Err(e) = self.inner.on_after_install(&bundle) {
            tracing::error!(
                "resource plugin '{}' on_after_install failed: {}",
                self.manifest.id,
                e
            );
        }
    }

    async fn on_activate(&self, _ctx: &PluginContext) -> PluginResult<()> {
        // Resource landing happens through the contribution bridge
        // (`WfPluginBridge::sync_all`) right after `register_contributions`.
        Ok(())
    }

    async fn on_deactivate(&self, _ctx: &PluginContext) -> PluginResult<()> {
        if let Err(e) = self.inner.on_after_uninstall() {
            tracing::error!(
                "resource plugin '{}' on_after_uninstall failed: {}",
                self.manifest.id,
                e
            );
        }
        Ok(())
    }
}

/// Register all built-in resource plugins on the plugin engine and activate
/// the ones requested by `RegisterOptions::resource_plugin_activation`.
pub async fn activate_builtin_resource_plugins_via_engine(
    engine: &wf_plugin::PluginEngine,
    opts: &wf_resource::registry::RegisterOptions,
) -> RuntimeResult<()> {
    for plugin in builtin_resource_plugins() {
        let meta = plugin.metadata();
        let config = opts
            .resource_plugin_activation
            .iter()
            .find(|sa| sa.id == meta.id)
            .map(|sa| sa.config.clone())
            .unwrap_or(Value::Null);
        let adapter = ResourcePluginAdapter::new(Arc::from(plugin), config);
        engine
            .register_plugin(adapter.manifest().clone(), Arc::new(adapter))
            .map_err(|e| {
                crate::error::RuntimeError::Config(format!(
                    "failed to register built-in resource plugin '{}': {e}",
                    meta.id
                ))
            })?;
    }

    for sa in &opts.resource_plugin_activation {
        if !engine.registry().has(&sa.id) {
            tracing::warn!(
                "resource_plugin_activation references unknown plugin '{}'",
                sa.id
            );
            continue;
        }
        engine.activate(&sa.id).await.map_err(|e| {
            crate::error::RuntimeError::Config(format!(
                "failed to activate built-in resource plugin '{}': {e}",
                sa.id
            ))
        })?;
    }

    Ok(())
}
