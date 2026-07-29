use std::sync::Arc;
use serde_json::Value;

use crate::contributions::manager::ContributionManager;

#[derive(Clone)]
pub struct PluginContext {
    pub plugin_id: String,
    pub config: Value,
    pub logger: PluginLogger,
    pub contribution_manager: Arc<ContributionManager>,
}

#[derive(Clone)]
pub struct PluginLogger;

impl PluginLogger {
    pub fn info(&self, msg: &str, args: Vec<String>) {
        if args.is_empty() {
            tracing::info!("{}", msg);
        } else {
            tracing::info!("{} {:?}", msg, args);
        }
    }
    pub fn warn(&self, msg: &str, args: Vec<String>) {
        if args.is_empty() {
            tracing::warn!("{}", msg);
        } else {
            tracing::warn!("{} {:?}", msg, args);
        }
    }
    pub fn error(&self, msg: &str, args: Vec<String>) {
        if args.is_empty() {
            tracing::error!("{}", msg);
        } else {
            tracing::error!("{} {:?}", msg, args);
        }
    }
    pub fn debug(&self, msg: &str, args: Vec<String>) {
        if args.is_empty() {
            tracing::debug!("{}", msg);
        } else {
            tracing::debug!("{} {:?}", msg, args);
        }
    }
}
