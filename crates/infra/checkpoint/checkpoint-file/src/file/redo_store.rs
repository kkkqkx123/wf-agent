//! Durable per-actor redo stacks for edit refs.
//!
//! Undo moves an actor's edit ref back one commit and records the removed
//! id; redo replays it. The stack is memory-cached and mirrored to SQLite so
//! an undone edit can still be redone after a restart. A per-actor cap
//! bounds database growth; the oldest entries are dropped first.

use std::sync::Arc;

use dashmap::DashMap;

use crate::storage::SqliteStorage;
use checkpoint_base::error::CheckpointError;

/// Per-actor redo depth kept in memory and on disk. A deeper stack has no
/// practical use; the cap keeps a runaway undo history from growing the
/// metadata database without bound.
pub(crate) const MAX_REDO_ENTRIES: usize = 50;

/// Per-actor redo history with a durable mirror.
pub(crate) struct RedoStackStore {
    stacks: DashMap<String, Vec<String>>,
    storage: Arc<SqliteStorage>,
}

impl RedoStackStore {
    /// Build a store backed by `storage`, loading every persisted stack into
    /// memory so redo survives restarts.
    pub(crate) fn new(storage: Arc<SqliteStorage>) -> Result<Self, CheckpointError> {
        let store = Self {
            stacks: DashMap::new(),
            storage,
        };
        for (actor, entries) in store.storage.load_all_redo_stacks()? {
            if !entries.is_empty() {
                store.stacks.insert(actor, entries);
            }
        }
        Ok(store)
    }

    /// Record one undone commit id for the actor and persist the stack. The
    /// memory mirror is updated first so it stays consistent with the ref
    /// move even when the durable write fails (the error still surfaces).
    pub(crate) fn push(&self, actor: &str, id: String) -> Result<(), CheckpointError> {
        self.push_many(actor, std::iter::once(id))
    }

    /// Record a run of undone commit ids (newest last) and persist once.
    pub(crate) fn push_many(
        &self,
        actor: &str,
        ids: impl IntoIterator<Item = String>,
    ) -> Result<(), CheckpointError> {
        let snapshot = {
            let mut entry = self.stacks.entry(actor.to_string()).or_default();
            for id in ids {
                entry.push(id);
            }
            if entry.len() > MAX_REDO_ENTRIES {
                let excess = entry.len() - MAX_REDO_ENTRIES;
                entry.drain(0..excess);
            }
            entry.value().clone()
        };
        self.storage.store_redo_stack(actor, &snapshot)?;
        Ok(())
    }

    /// The most recently undone commit id, without removing it.
    pub(crate) fn last(&self, actor: &str) -> Option<String> {
        self.stacks
            .get(actor)
            .and_then(|stack| stack.last().cloned())
    }

    /// Remove and return the most recently undone commit id, persisting the
    /// shortened stack. The memory mirror is updated before the durable
    /// write so a store failure never leaves the process out of sync.
    pub(crate) fn pop(&self, actor: &str) -> Result<Option<String>, CheckpointError> {
        let (popped, snapshot) = match self.stacks.get_mut(actor) {
            Some(mut entry) => {
                let popped = entry.pop();
                (popped, entry.value().clone())
            }
            None => return Ok(None),
        };
        if popped.is_some() {
            self.storage.store_redo_stack(actor, &snapshot)?;
        }
        Ok(popped)
    }

    /// Whether any redo is available for the actor.
    pub(crate) fn is_available(&self, actor: &str) -> bool {
        self.stacks
            .get(actor)
            .is_some_and(|stack| !stack.is_empty())
    }

    /// Discard the actor's redo history (a new commit supersedes it).
    pub(crate) fn clear(&self, actor: &str) -> Result<(), CheckpointError> {
        if self.stacks.remove(actor).is_none() {
            return Ok(());
        }
        self.storage.clear_stored_redo_stack(actor)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> RedoStackStore {
        RedoStackStore::new(Arc::new(SqliteStorage::new_full_in_memory().unwrap())).unwrap()
    }

    #[test]
    fn push_pop_and_cap_keep_newest() {
        let store = store();
        for i in 0..(MAX_REDO_ENTRIES + 5) {
            store.push("agent:a", format!("c{i}")).unwrap();
        }
        assert_eq!(
            store.last("agent:a"),
            Some(format!("c{}", MAX_REDO_ENTRIES + 4))
        );
        assert_eq!(
            store.pop("agent:a").unwrap(),
            Some(format!("c{}", MAX_REDO_ENTRIES + 4))
        );
    }

    #[test]
    fn clear_drops_history() {
        let store = store();
        store.push("agent:a", "c1".to_string()).unwrap();
        assert!(store.is_available("agent:a"));
        store.clear("agent:a").unwrap();
        assert!(!store.is_available("agent:a"));
        assert!(store.pop("agent:a").unwrap().is_none());
    }
}
