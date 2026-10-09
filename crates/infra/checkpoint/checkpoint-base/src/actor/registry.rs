//! Actor resolution cache extracted from `FileCheckpointManager`.
//!
//! Entity id -> `ActorId` caching lives here so actor hierarchy rules can
//! evolve without touching storage, scan or merge code.

use std::sync::Arc;

use dashmap::DashMap;

use crate::actor::id::ActorId;

/// Thread-safe entity id -> resolved actor cache.
#[derive(Debug, Clone, Default)]
pub struct ActorCache {
    inner: Arc<DashMap<String, ActorId>>,
}

impl ActorCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, entity_id: &str) -> Option<ActorId> {
        self.inner.get(entity_id).map(|a| a.clone())
    }

    pub fn insert(&self, entity_id: impl Into<String>, actor: ActorId) {
        self.inner.insert(entity_id.into(), actor);
    }

    pub fn remove(&self, entity_id: &str) {
        self.inner.remove(entity_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::id::{ActorId, ActorKind};

    #[test]
    fn cache_roundtrip() {
        let cache = ActorCache::new();
        assert!(cache.get("e1").is_none());
        let actor = ActorId::new(ActorKind::Agent, &[wf_types::Id::from("e1")]).unwrap();
        cache.insert("e1", actor.clone());
        assert_eq!(cache.get("e1"), Some(actor));
        cache.remove("e1");
        assert!(cache.get("e1").is_none());
    }
}
