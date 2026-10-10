use std::collections::HashMap;

use crate::file::git_write::map_git_error;
use crate::file::FileCheckpointManager;
pub use crate::precise::{PreciseApplyStats, PreciseFileEvent, PreciseFileEventKind};
use checkpoint_base::actor::id::{ActorId, ActorKind};
use checkpoint_base::error::CheckpointError;

/// Actor-partition facade for `FileCheckpointManager`.
// Partition lifecycle and edit primitives live here; precise event types
// are defined in `crate::precise` and re-exported above for compatibility.
impl FileCheckpointManager {
    /// Resolve the actor partition for a child execution: full `ActorId`
    /// strings parse as-is; otherwise a child actor is derived from the
    /// given parent (`parent.child(execution_id)`) so nested executions are
    /// isolated in their own partition.
    pub fn actor_id_for_child(
        &self,
        entity_id: &str,
        parent: Option<&ActorId>,
    ) -> Result<ActorId, CheckpointError> {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return Ok(actor.clone());
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return Ok(actor);
        }
        let child_id = wf_types::Id::from(entity_id.to_string());
        let actor = match parent {
            Some(parent) => parent
                .child(&child_id)
                .map_err(|e| CheckpointError::Validation {
                    reason: format!("invalid child actor for '{entity_id}': {e}"),
                }),
            None => ActorId::new(ActorKind::Agent, &[child_id]).map_err(|e| {
                CheckpointError::Validation {
                    reason: format!("invalid actor for '{entity_id}': {e}"),
                }
            }),
        }?;
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        Ok(actor)
    }

    /// Resolve the actor for an execution entity id. Full `ActorId` strings
    /// (e.g. `agent:{loop_id}` / `wf:{workflow_id}/child:{subgraph_id}`)
    /// parse as-is; bare execution ids map to a root agent actor. A
    /// previously resolved hierarchical actor (via [`Self::resolve_actor`])
    /// takes precedence so later calls keep the same partition.
    pub fn actor_id_for(&self, entity_id: &str) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        let actor = match ActorId::parse(entity_id) {
            Ok(actor) => actor,
            Err(_) => crate::file::util::root_actor(wf_types::Id::from(entity_id.to_string())),
        };
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        actor
    }

    /// Resolve the actor for an execution with an optional immediate parent
    /// (sub-execution isolation). A child execution whose parent is
    /// already in the actor index is encoded as `parent.child(execution_id)`
    /// (`{kind}:{parent}/child:{child}`), keeping nested executions in their
    /// own hierarchical partition. Falls back to a root actor when the
    /// parent is unknown.
    pub fn resolve_actor(&self, entity_id: &str, parent_execution_id: Option<&str>) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return actor;
        }
        let child_id = wf_types::Id::from(entity_id.to_string());
        let actor = match parent_execution_id {
            Some(parent) if parent != entity_id => match self.actor_index.get(parent) {
                Some(parent_actor) => parent_actor
                    .child(&child_id)
                    .unwrap_or_else(|_| crate::file::util::root_actor(child_id.clone())),
                None => crate::file::util::root_actor(child_id.clone()),
            },
            _ => crate::file::util::root_actor(child_id.clone()),
        };
        self.actor_index
            .insert(entity_id.to_string(), actor.clone());
        actor
    }

    /// Resolve the actor using the full ancestor chain when available.
    /// Unlike [`Self::resolve_actor`], a cold index does not collapse to the
    /// root: the caller-supplied `ancestors` (root-to-parent, oldest first,
    /// excluding self) are used verbatim with the entity id appended, so deep
    /// hierarchies keep full ancestry after a restart. Falls back to
    /// [`Self::resolve_actor`] when the chain is empty.
    pub fn resolve_actor_with_chain(
        &self,
        entity_id: &str,
        ancestors: &[String],
        parent_execution_id: Option<&str>,
    ) -> ActorId {
        if let Some(actor) = self.actor_index.get(entity_id) {
            return actor.clone();
        }
        if let Ok(actor) = ActorId::parse(entity_id) {
            self.actor_index
                .insert(entity_id.to_string(), actor.clone());
            return actor;
        }
        if !ancestors.is_empty() {
            let mut chain = ancestors.to_vec();
            if chain.last().map(String::as_str) != Some(entity_id) {
                chain.push(entity_id.to_string());
            }
            let kind = parent_execution_id
                .and_then(|p| self.actor_index.get(p))
                .and_then(|a| a.try_kind().ok())
                .unwrap_or(ActorKind::Agent);
            let chain_ids: Vec<wf_types::Id> = chain;
            if let Ok(actor) = ActorId::new(kind, &chain_ids) {
                self.actor_index
                    .insert(entity_id.to_string(), actor.clone());
                return actor;
            }
        }
        self.resolve_actor(entity_id, parent_execution_id)
    }

    /// The resolved actor of an entity, if it was resolved earlier.
    pub fn resolved_actor(&self, entity_id: &str) -> Option<ActorId> {
        self.actor_index.get(entity_id)
    }

    // ── actor edit-line primitives ──────────────────────────────────

    /// Ensure the actor's edit line exists. Validates the actor identity so
    /// later edits have a stable partition home.
    pub fn ensure_agent_partition(&self, actor: &ActorId) -> Result<(), CheckpointError> {
        actor.try_kind().map_err(|e| CheckpointError::Validation {
            reason: format!("invalid actor partition '{actor}': {e}"),
        })?;
        if actor.hierarchy().is_empty() {
            return Err(CheckpointError::Validation {
                reason: format!("actor partition '{actor}' has empty hierarchy"),
            });
        }
        Ok(())
    }

    /// Record one file edit for an actor: the in-memory bytes are committed
    /// directly on the actor's edit ref (no disk re-read). Returns the new
    /// commit id (hex).
    pub fn apply_agent_edit(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
    ) -> Result<String, CheckpointError> {
        self.apply_agent_edit_with_hash(actor, path, content, None)
    }

    /// Hash-aware edit entry: when `expected_hash` is present the current
    /// content hash must match before writing, otherwise a conflict error is
    /// reported. Content identity remains the object id.
    pub fn apply_agent_edit_with_hash(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
        expected_hash: Option<&str>,
    ) -> Result<String, CheckpointError> {
        let path = crate::file::util::validate_workspace_relative_path(path)?;
        if let Some(expected) = expected_hash {
            let git = self.git_ref()?;
            let edit_ref = crate::git_store::edit_ref_for_actor(actor.as_str());
            let actual = match git
                .read_ref(&edit_ref)
                .map_err(crate::file::git_write::map_git_error)?
            {
                Some(head) => {
                    let commit = git
                        .read_commit(&head)
                        .map_err(crate::file::git_write::map_git_error)?;
                    let files = git
                        .tree_to_bytes(&commit.tree)
                        .map_err(crate::file::git_write::map_git_error)?;
                    match files.get(&path) {
                        Some(bytes) => crate::file::util::sha256_hex(bytes),
                        None => String::new(),
                    }
                }
                None => String::new(),
            };
            if actual != expected {
                return Err(CheckpointError::Validation {
                    reason: format!(
                        "hash mismatch for '{path}': expected {expected}, got {actual}"
                    ),
                });
            }
        }
        let mut files = HashMap::new();
        files.insert(path.clone(), Some(content.to_vec()));
        let outcome = self.commit_tool_files(actor, &files, None, Some("tool"), "tool edit")?;
        Ok(outcome.id)
    }

    /// Record one file deletion for an actor: commit the deletion on the
    /// actor's edit ref. Returns the new commit id (hex).
    pub fn apply_agent_delete(
        &self,
        actor: &ActorId,
        path: &str,
    ) -> Result<String, CheckpointError> {
        let path = crate::file::util::validate_workspace_relative_path(path)?;
        let mut files = HashMap::new();
        files.insert(path.clone(), None);
        let outcome = self.commit_tool_files(actor, &files, None, Some("tool"), "tool delete")?;
        Ok(outcome.id)
    }

    /// Record a manual (human/IDE) edit on the human ref, bypassing any
    /// actor. Human commits are never auto-merged. Returns the new commit
    /// id (hex).
    pub fn apply_manual_edit(&self, path: &str, content: &[u8]) -> Result<String, CheckpointError> {
        let path = crate::file::util::validate_workspace_relative_path(path)?;
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut changes = HashMap::new();
        changes.insert(
            path.clone(),
            Some((crate::git_store::MODE_FILE.to_string(), content.to_vec())),
        );
        let message = crate::git_store::commit_message(
            "manual edit",
            Some("human"),
            None,
            Some("manual"),
            &[],
        );
        let outcome = git
            .commit_on_ref(crate::git_store::REF_HUMAN, &changes, "human", &message)
            .map_err(map_git_error)?;
        if outcome.created {
            self.index_commit(
                storage,
                &outcome.id,
                "human",
                "",
                "manual",
                std::slice::from_ref(&path),
            )?;
            self.publish_file_event(&outcome.id, &path, "manual", Some(content));
        }
        Ok(outcome.id)
    }

    /// Record a manual (human/IDE) file deletion on the human ref.
    /// Returns the new commit id (hex).
    pub fn apply_manual_delete(&self, path: &str) -> Result<String, CheckpointError> {
        let path = crate::file::util::validate_workspace_relative_path(path)?;
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut changes = HashMap::new();
        changes.insert(path.clone(), None);
        let message = crate::git_store::commit_message(
            "manual delete",
            Some("human"),
            None,
            Some("manual"),
            &[],
        );
        let outcome = git
            .commit_on_ref(crate::git_store::REF_HUMAN, &changes, "human", &message)
            .map_err(map_git_error)?;
        if outcome.created {
            self.index_commit(
                storage,
                &outcome.id,
                "human",
                "",
                "manual",
                std::slice::from_ref(&path),
            )?;
            self.publish_file_event(&outcome.id, &path, "manual", None);
        }
        Ok(outcome.id)
    }

    /// Discard an execution's file changes: delete the actor's edit ref and
    /// drop the in-memory projection mirrors. Commits stay reachable from
    /// other refs (or become unreachable and age out via object-store
    /// cleanup); merged history is never rewritten. No-op when the actor
    /// has no edit ref.
    pub fn discard_execution(&self, entity_id: &str) -> Result<(), CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        git.delete_ref(&crate::git_store::edit_ref_for_actor(actor.as_str()))
            .map_err(map_git_error)?;
        self.store.latest_checkpoints.remove(actor.as_str());
        self.redo_stacks.remove(actor.as_str());
        Ok(())
    }

    /// Explicit rename entry point: delete the old path and write the new
    /// path content in a single atomic commit on the actor's edit ref.
    /// Rename following happens on read via similarity detection. Returns
    /// the new commit id (hex).
    pub fn rename_file(
        &self,
        actor: &ActorId,
        from_path: &str,
        to_path: &str,
        content: &[u8],
    ) -> Result<String, checkpoint_base::error::CheckpointError> {
        let from = crate::file::util::validate_workspace_relative_path(from_path)?;
        let to = crate::file::util::validate_workspace_relative_path(to_path)?;
        let mut files = HashMap::new();
        files.insert(from, None);
        files.insert(to, Some(content.to_vec()));
        self.commit_tool_files(actor, &files, None, Some("tool"), "rename")
            .map(|o| o.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::FileContentEntry;

    fn entry(path: &str, content: &[u8]) -> FileContentEntry {
        FileContentEntry::new(path, content.to_vec())
    }

    #[test]
    fn child_actor_stays_isolated_without_pointer_index() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let parent_cp = manager
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();

        let child = manager.resolve_actor("child-1", Some("parent-1"));
        let parent_actor = manager.actor_id_for("parent-1");
        assert_ne!(child.as_str(), parent_actor.as_str());
        assert_eq!(
            manager
                .latest_checkpoint_id(&parent_actor)
                .unwrap()
                .as_deref(),
            Some(parent_cp.id.as_str())
        );
    }

    #[tokio::test]
    async fn resolve_actor_with_chain_keeps_full_ancestry_on_cold_index() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.resolve_actor_with_chain(
            "leaf",
            &["root".to_string(), "mid".to_string()],
            Some("mid"),
        );
        assert_eq!(actor.as_str(), "agent:root/child:mid/child:leaf");
        assert_eq!(actor.hierarchy(), vec!["root", "mid", "leaf"]);
    }

    #[tokio::test]
    async fn resolve_actor_with_chain_falls_back_without_ancestors() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let chained = manager.resolve_actor_with_chain("leaf", &[], Some("parent"));
        let direct = manager.resolve_actor("other", Some("parent"));
        assert_eq!(chained.as_str(), direct.as_str().replace("other", "leaf"));
    }

    #[test]
    fn agent_edits_form_linear_commit_chain() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("entity-1");
        let first = manager.apply_agent_edit(&actor, "a.txt", b"one").unwrap();
        let second = manager.apply_agent_edit(&actor, "a.txt", b"two").unwrap();
        assert_ne!(first, second);
        let head = manager.latest_checkpoint_id(&actor).unwrap().unwrap();
        assert_eq!(head, second);
        let git = manager.git_ref().unwrap();
        assert!(git.is_ancestor(&first, &second).unwrap());
    }

    #[test]
    fn rename_commits_delete_plus_add_atomically() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("entity-1");
        manager
            .apply_agent_edit(&actor, "old.txt", b"data")
            .unwrap();
        let renamed = manager
            .rename_file(&actor, "old.txt", "new.txt", b"data")
            .unwrap();
        let workspace = manager.get_actor_workspace(actor.as_str()).unwrap();
        let paths: Vec<&str> = workspace.iter().map(|f| f.path.as_str()).collect();
        assert!(!paths.contains(&"old.txt"));
        assert!(paths.contains(&"new.txt"));
        assert_eq!(
            manager.latest_checkpoint_id(&actor).unwrap().as_deref(),
            Some(renamed.as_str())
        );
    }
}
