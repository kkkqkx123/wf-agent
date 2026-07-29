use std::sync::Arc;
use dashmap::DashMap;

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::plugin::Plugin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    Discovered,
    Loading,
    Loaded,
    Activating,
    Active,
    Deactivating,
    Deactivated,
    Error,
}

struct PluginRecord {
    manifest: PluginManifest,
    instance: Arc<dyn Plugin>,
    status: PluginStatus,
    error: Option<String>,
    activated_at: Option<i64>,
}

pub struct PluginRegistry {
    plugins: DashMap<String, PluginRecord>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { plugins: DashMap::new() }
    }

    pub fn register(&self, manifest: PluginManifest, instance: Arc<dyn Plugin>) -> PluginResult<()> {
        if self.plugins.contains_key(&manifest.id) {
            return Err(PluginError::AlreadyExists(manifest.id));
        }
        self.plugins.insert(manifest.id.clone(), PluginRecord {
            manifest,
            instance,
            status: PluginStatus::Discovered,
            error: None,
            activated_at: None,
        });
        Ok(())
    }

    pub fn get(&self, plugin_id: &str) -> Option<PluginInfo> {
        self.plugins.get(plugin_id).map(|r| PluginInfo {
            manifest: r.manifest.clone(),
            status: r.status,
            error: r.error.clone(),
            activated_at: r.activated_at,
        })
    }

    pub fn has(&self, plugin_id: &str) -> bool {
        self.plugins.contains_key(plugin_id)
    }

    pub fn instance(&self, plugin_id: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.get(plugin_id).map(|r| r.instance.clone())
    }

    pub fn update_status(&self, plugin_id: &str, status: PluginStatus) {
        if let Some(mut r) = self.plugins.get_mut(plugin_id) {
            r.status = status;
            if status == PluginStatus::Active {
                r.activated_at = Some(chrono::Utc::now().timestamp_millis());
            }
        }
    }

    pub fn set_error(&self, plugin_id: &str, err: impl Into<String>) {
        if let Some(mut r) = self.plugins.get_mut(plugin_id) {
            r.status = PluginStatus::Error;
            r.error = Some(err.into());
        }
    }

    pub fn remove(&self, plugin_id: &str) {
        self.plugins.remove(plugin_id);
    }

    pub fn list_by_status(&self, status: PluginStatus) -> Vec<PluginInfo> {
        self.plugins.iter()
            .filter(|r| r.status == status)
            .map(|r| PluginInfo {
                manifest: r.manifest.clone(),
                status: r.status,
                error: r.error.clone(),
                activated_at: r.activated_at,
            })
            .collect()
    }

    pub fn all(&self) -> Vec<PluginInfo> {
        self.plugins.iter().map(|r| PluginInfo {
            manifest: r.manifest.clone(),
            status: r.status,
            error: r.error.clone(),
            activated_at: r.activated_at,
        }).collect()
    }

    pub fn len(&self) -> usize { self.plugins.len() }
    pub fn is_empty(&self) -> bool { self.plugins.is_empty() }
    pub fn clear(&self) { self.plugins.clear(); }
}

impl Default for PluginRegistry {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub error: Option<String>,
    pub activated_at: Option<i64>,
}
