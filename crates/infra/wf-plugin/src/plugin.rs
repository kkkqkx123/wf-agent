use async_trait::async_trait;
use serde_json::Value;

use crate::context::PluginContext;
use crate::contributions::registrar::ContributionRegistrar;
use crate::error::PluginResult;
use crate::manifest::PluginManifest;

#[async_trait]
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    async fn on_load(&self, _ctx: &PluginContext) -> PluginResult<()> {
        Ok(())
    }
    async fn on_unload(&self, _ctx: &PluginContext) -> PluginResult<()> {
        Ok(())
    }
    async fn on_activate(&self, _ctx: &PluginContext) -> PluginResult<()> {
        Ok(())
    }
    async fn on_deactivate(&self, _ctx: &PluginContext) -> PluginResult<()> {
        Ok(())
    }
    async fn on_config_change(&self, _config: &Value) -> PluginResult<()> {
        Ok(())
    }
    /// Register the plugin's contributions. A returned error aborts
    /// activation: propagate registrar errors with `?` instead of dropping
    /// them so `Forbid`-policy conflicts fail loudly.
    fn register_contributions(
        &self,
        _registrar: &mut dyn ContributionRegistrar,
    ) -> PluginResult<()> {
        Ok(())
    }
}
