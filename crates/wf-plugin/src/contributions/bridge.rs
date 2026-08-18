use async_trait::async_trait;

use super::manager::ContributionManager;
use crate::error::PluginResult;

#[async_trait]
pub trait ContributionBridge: Send + Sync {
    async fn sync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()>;
    async fn unsync_all(&self, plugin_id: &str, manager: &ContributionManager) -> PluginResult<()>;
}
