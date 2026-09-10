//! Actor resolution index extracted from `FileCheckpointManager`.
//!
//! First step of splitting the manager god-object: entity id -> `ActorId`
//! caching lives here so actor hierarchy rules can evolve without touching
//! storage, scan or merge code.

use std::sync::Arc;

use dashmap::DashMap;

use crate::actor_id::ActorId;

/// Thread-safe entity id -> resolved actor cache.
#[derive(Debug, Clone, Default)]
pub struct ActorRegistry {
    inner: Arc<DashMap<String, ActorId>>,
}

impl ActorRegistry {
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

    /// Direct access for migration-era call sites still using map APIs.
    pub fn as_map(&self) -> &DashMap<String, ActorId> {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_id::{ActorId, ActorKind};

    #[test]
    fn registry_roundtrip() {
        let reg = ActorRegistry::new();
        assert!(reg.get("e1").is_none());
        let actor = ActorId::new(ActorKind::Agent, &[wf_types::Id::from("e1")]).unwrap();
        reg.insert("e1", actor.clone());
        assert_eq!(reg.get("e1"), Some(actor));
        reg.remove("e1");
        assert!(reg.get("e1").is_none());
    }
}
