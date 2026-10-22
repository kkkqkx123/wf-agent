//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.



mod approve;
mod conflict;
mod list;
mod merge;
mod policy;
mod reject;

#[cfg(test)]
mod tests {
    use super::*;

    fn orphan_ref(manager: &FileCheckpointManager, entity: &str, tag: &str) -> String {
        let actor = manager.actor_id_for(entity);
        format!(
            "refs/wf/review/{}-{tag}-0",
            crate::git_store::sanitize_ref_component(actor.as_str())
        )
    }

    #[test]
    fn reconcile_sweeps_only_actor_orphans() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let storage = manager.storage_ref().unwrap();
        let alice_row = orphan_ref(&manager, "alice", "7");
        storage
            .set_review_state(&alice_row, ReviewStatus::Pending)
            .unwrap();
        storage
            .set_review_state("refs/wf/review/bob-7-0", ReviewStatus::Pending)
            .unwrap();
        assert_eq!(manager.reconcile_reviews("alice").unwrap(), 1);
        assert_eq!(storage.get_review_state(&alice_row).unwrap(), None);
        assert_eq!(
            storage.get_review_state("refs/wf/review/bob-7-0").unwrap(),
            Some(ReviewStatus::Pending)
        );
    }

    #[test]
    fn reject_converges_after_orphan_sweep() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let storage = manager.storage_ref().unwrap();
        let alice_row = orphan_ref(&manager, "alice", "9");
        storage
            .set_review_state(&alice_row, ReviewStatus::Pending)
            .unwrap();
        let first = manager.reject_changes("alice", None);
        assert!(first.is_err());
        assert_eq!(storage.get_review_state(&alice_row).unwrap(), None);
        let second = manager.reject_changes("alice", None);
        assert!(second.is_err());
    }
}
