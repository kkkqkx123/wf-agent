//! Child-parent relationship resolution: in-memory index plus an optional
//! caching decorator.

use dashmap::DashMap;
use std::sync::Arc;

pub trait ChildCheckpointResolver: Send + Sync {
    fn resolve_children(&self, parent_id: &str) -> Vec<String>;
    fn resolve_parent(&self, child_id: &str) -> Option<String>;
}

pub struct InMemoryChildResolver {
    parent_to_children: DashMap<String, Vec<String>>,
    child_to_parent: DashMap<String, String>,
}

impl InMemoryChildResolver {
    pub fn new() -> Self {
        Self {
            parent_to_children: DashMap::new(),
            child_to_parent: DashMap::new(),
        }
    }

    pub fn register_relationship(&self, parent_id: &str, child_id: &str) {
        let mut entry = self
            .parent_to_children
            .entry(parent_id.to_string())
            .or_default();
        if !entry.iter().any(|id| id == child_id) {
            entry.push(child_id.to_string());
        }
        self.child_to_parent
            .insert(child_id.to_string(), parent_id.to_string());
    }

    pub fn register_batch(&self, relationships: &[(String, String)]) {
        for (parent, child) in relationships {
            self.register_relationship(parent, child);
        }
    }
}

impl Default for InMemoryChildResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildCheckpointResolver for InMemoryChildResolver {
    fn resolve_children(&self, parent_id: &str) -> Vec<String> {
        self.parent_to_children
            .get(parent_id)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    fn resolve_parent(&self, child_id: &str) -> Option<String> {
        self.child_to_parent.get(child_id).map(|v| v.clone())
    }
}

pub struct CachedChildResolver {
    inner: Arc<dyn ChildCheckpointResolver>,
    cache: DashMap<String, Vec<String>>,
    parent_cache: DashMap<String, Option<String>>,
}

impl CachedChildResolver {
    pub fn new(inner: Arc<dyn ChildCheckpointResolver>) -> Self {
        Self {
            inner,
            cache: DashMap::new(),
            parent_cache: DashMap::new(),
        }
    }

    pub fn invalidate(&self, parent_id: &str) {
        cache_remove(&self.cache, parent_id);
    }

    pub fn invalidate_parent(&self, child_id: &str) {
        self.parent_cache.remove(child_id);
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
        self.parent_cache.clear();
    }
}

fn cache_remove(cache: &DashMap<String, Vec<String>>, key: &str) {
    cache.remove(key);
}

impl ChildCheckpointResolver for CachedChildResolver {
    fn resolve_children(&self, parent_id: &str) -> Vec<String> {
        if let Some(cached) = self.cache.get(parent_id) {
            return cached.clone();
        }
        let children = self.inner.resolve_children(parent_id);
        self.cache.insert(parent_id.to_string(), children.clone());
        children
    }

    fn resolve_parent(&self, child_id: &str) -> Option<String> {
        if let Some(cached) = self.parent_cache.get(child_id) {
            return cached.clone();
        }
        let parent = self.inner.resolve_parent(child_id);
        self.parent_cache
            .insert(child_id.to_string(), parent.clone());
        parent
    }
}
