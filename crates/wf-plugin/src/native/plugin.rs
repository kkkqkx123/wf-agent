use std::sync::Arc;

use async_trait::async_trait;

use crate::contributions::registrar::ContributionRegistrar;
use crate::error::PluginResult;
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

pub struct NativePlugin {
    manifest: PluginManifest,
    _lib: Arc<libloading::Library>,
}

impl NativePlugin {
    pub fn new(manifest: PluginManifest, lib: libloading::Library) -> Self {
        Self { manifest, _lib: Arc::new(lib) }
    }
}

#[async_trait]
impl Plugin for NativePlugin {
    fn manifest(&self) -> &PluginManifest { &self.manifest }

    async fn on_load(&self, _ctx: &crate::context::PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_unload(&self, _ctx: &crate::context::PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_activate(&self, _ctx: &crate::context::PluginContext) -> PluginResult<()> { Ok(()) }
    async fn on_deactivate(&self, _ctx: &crate::context::PluginContext) -> PluginResult<()> { Ok(()) }

    fn register_contributions(&self, _registrar: &mut dyn ContributionRegistrar) {}
}
