use std::path::PathBuf;
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

#[derive(Debug, Clone)]
pub struct ContributionRecord {
    pub contribution_type: String,
    pub key: String,
    pub plugin_id: String,
}

struct PluginRecord {
    manifest: PluginManifest,
    instance: Arc<dyn Plugin>,
    status: PluginStatus,
    error: Option<String>,
    activated_at: Option<i64>,
    contributions: Vec<ContributionRecord>,
}

impl PluginRecord {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            manifest: self.manifest.clone(),
            status: self.status,
            error: self.error.clone(),
            activated_at: self.activated_at,
            contributions: self.contributions.clone(),
        }
    }
}

pub struct PluginRegistry {
    plugins: DashMap<String, PluginRecord>,
    contributions_index: DashMap<String, Vec<ContributionRecord>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: DashMap::new(),
            contributions_index: DashMap::new(),
        }
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
            contributions: Vec::new(),
        });
        Ok(())
    }

    pub fn get(&self, plugin_id: &str) -> Option<PluginInfo> {
        self.plugins.get(plugin_id).map(|r| r.info())
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

    pub fn add_contributions(&self, plugin_id: &str, contributions: Vec<ContributionRecord>) {
        if let Some(mut r) = self.plugins.get_mut(plugin_id) {
            for c in &contributions {
                self.contributions_index
                    .entry(c.contribution_type.clone())
                    .or_default()
                    .push(c.clone());
            }
            r.contributions.extend(contributions);
        }
    }

    pub fn list_by_contribution(&self, contribution_type: &str) -> Vec<ContributionRecord> {
        self.contributions_index
            .get(contribution_type)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    pub fn remove(&self, plugin_id: &str) {
        if let Some((_, record)) = self.plugins.remove(plugin_id) {
            for c in &record.contributions {
                if let Some(mut entries) = self.contributions_index.get_mut(&c.contribution_type) {
                    entries.retain(|e| e.plugin_id != plugin_id);
                }
            }
            self.contributions_index.retain(|_, v| !v.is_empty());
        }
    }

    pub fn list_by_status(&self, status: PluginStatus) -> Vec<PluginInfo> {
        self.plugins.iter()
            .filter(|r| r.status == status)
            .map(|r| r.info())
            .collect()
    }

    pub fn all(&self) -> Vec<PluginInfo> {
        self.plugins.iter().map(|r| r.info()).collect()
    }

    pub fn len(&self) -> usize { self.plugins.len() }
    pub fn is_empty(&self) -> bool { self.plugins.is_empty() }
    pub fn clear(&self) {
        self.plugins.clear();
        self.contributions_index.clear();
    }
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
    pub contributions: Vec<ContributionRecord>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub manifest: PluginManifest,
    pub source_path: PathBuf,
}
