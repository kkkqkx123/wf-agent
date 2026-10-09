use std::collections::HashMap;

use crate::branch::{execution_pointer_name, manager::ExecutionPointerAdapter};
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

    /// Ensure the child execution's branch has been created. Called by
    /// `prepare_with_parent` to set up the branch isolation before any
    /// checkpoint activity. No-op when the entity has no parent or is the
    /// parent itself.
    pub async fn ensure_child_branch(
        &self,
        entity_id: &str,
        parent_execution_id: Option<&str>,
    ) -> Result<(), CheckpointError> {
        let Some(parent) = parent_execution_id else {
            return Ok(());
        };
        if parent == entity_id {
            return Ok(());
        }
        let branch_name = execution_pointer_name(entity_id);
        if self
            .store
            .pointer_adapter
            .branch_exists(&branch_name)
            .await?
        {
            return Ok(());
        }
        let parent_branch = execution_pointer_name(parent);
        let base = if self
            .store
            .pointer_adapter
            .branch_exists(&parent_branch)
            .await?
        {
            Some(parent_branch)
        } else {
            None
        };
        self.store
            .pointer_adapter
            .create_branch(&branch_name, base.as_deref())
            .await?;
        Ok(())
    }

    // ── actor edit-line primitives ──────────────────────────────────

    /// Ensure the actor's edit line exists. There are no partitions in the
    /// Git model; the edit ref is created lazily by the first commit, so
    /// this is always a no-op success kept for call-site stability.
    pub fn ensure_agent_partition(&self, actor: &ActorId) -> Result<(), CheckpointError> {
        let _ = actor;
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

    /// Hash-aware edit entry: `expected_hash` is accepted for call-site
    /// stability and ignored — content identity is the object id, and
    /// attribution no longer consults a hash registry.
    pub fn apply_agent_edit_with_hash(
        &self,
        actor: &ActorId,
        path: &str,
        content: &[u8],
        _expected_hash: Option<&str>,
    ) -> Result<String, CheckpointError> {
        let path = crate::file::util::validate_workspace_relative_path(path)?;
        let mut files = HashMap::new();
        files.insert(path.clone(), Some(content.to_vec()));
        let outcome = self.commit_tool_files(actor, &files, None, None, "tool edit")?;
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
        let outcome = self.commit_tool_files(actor, &files, None, None, "tool delete")?;
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
        let message =
            crate::git_store::commit_message("manual edit", Some("human"), None, None, &[]);
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
        let message =
            crate::git_store::commit_message("manual delete", Some("human"), None, None, &[]);
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

    /// Record a file move/rename linkage. Renames are detected on read via
    /// content similarity, so this validates both sides and succeeds
    /// without persisting anything. Kept for call-site stability.
    pub fn track_file_move(
        &self,
        from_path: &str,
        to_path: &str,
        source: &str,
    ) -> Result<(), checkpoint_base::error::CheckpointError> {
        let _ = source;
        crate::file::util::validate_workspace_relative_path(from_path)?;
        crate::file::util::validate_workspace_relative_path(to_path)?;
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
        self.commit_tool_files(actor, &files, None, None, "rename")
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

    #[tokio::test]
    async fn child_execution_creates_branch() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();

        let branch = execution_pointer_name("child-1");
        assert!(
            !manager
                .store
                .pointer_adapter
                .branch_exists(&branch)
                .await
                .unwrap(),
            "branch must not exist before the child is prepared"
        );

        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();
        assert!(manager
            .store
            .pointer_adapter
            .branch_exists(&branch)
            .await
            .unwrap());

        // Idempotent: preparing the same child again keeps the branch set
        // stable. The parent stays branchless: checkpoint creation only
        // advances explicitly prepared execution branches and never
        // implicitly registers one for root executions.
        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();
        assert!(
            manager
                .store
                .pointer_adapter
                .branch_exists(&branch)
                .await
                .unwrap(),
            "re-preparing keeps the child branch"
        );
        assert!(
            !manager
                .store
                .pointer_adapter
                .branch_exists(&execution_pointer_name("parent-1"))
                .await
                .unwrap(),
            "the parent stays branchless"
        );
    }

    #[tokio::test]
    async fn duplicate_branch_rejected() {
        use crate::branch::ExecutionPointerAdapter;

        let manager = FileCheckpointManager::new_in_memory().unwrap();
        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();
        let branch = execution_pointer_name("child-1");
        let err = manager
            .store
            .pointer_adapter
            .create_branch(&branch, None)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            checkpoint_base::error::CheckpointError::Branch(_)
        ));
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

    #[tokio::test]
    async fn ensure_child_branch_ignores_roots_and_self_parent() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();

        // No parent (root execution) and self-parent are no-ops.
        manager.ensure_child_branch("solo", None).await.unwrap();
        manager
            .ensure_child_branch("same", Some("same"))
            .await
            .unwrap();

        for entity in ["solo", "same"] {
            assert!(
                !manager
                    .store
                    .pointer_adapter
                    .branch_exists(&execution_pointer_name(entity))
                    .await
                    .unwrap(),
                "no branch may be created for '{entity}'"
            );
        }
    }

    #[tokio::test]
    async fn child_branch_starts_headless_after_fork() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let parent_cp = manager
            .create_checkpoint("parent-1", &[entry("a.txt", b"base")])
            .unwrap();

        manager
            .ensure_child_branch("child-1", Some("parent-1"))
            .await
            .unwrap();

        // The forked branch exists natively but stays headless until its own
        // first checkpoint; the parent base remains readable as the fork
        // point without a KV registry entry.
        let branch = execution_pointer_name("child-1");
        assert!(
            manager
                .store
                .pointer_adapter
                .branch_exists(&branch)
                .await
                .unwrap(),
            "forked branch must exist"
        );
        assert_eq!(
            manager
                .store
                .pointer_adapter
                .get_branch_head(&branch)
                .unwrap(),
            None,
            "forked branch stays headless until its own checkpoint"
        );
        let parent_actor = manager.actor_id_for("parent-1");
        assert_eq!(
            manager
                .latest_checkpoint_id(&parent_actor)
                .unwrap()
                .as_deref(),
            Some(parent_cp.id.as_str()),
            "parent base stays readable as the fork point"
        );
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
