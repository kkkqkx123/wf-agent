use std::collections::{HashMap, HashSet};

use wf_types::checkpoint::CheckpointType;
use wf_types::storage::CheckpointStorageMetadata;

/// Dependency guard over an entity's execution-state checkpoint chain.
/// Division of labor: this guard protects `wf-storage` rows only and tracks a
/// linear previous-id chain keyed by opaque string ids. File-history ancestry
/// is a separate multi-parent content-id graph in the Git object store
/// with its own mark-sweep; the two graphs never share
/// nodes and must not be merged into a generic graph.
///
/// - `referenced_by` maps each checkpoint id to the ids of checkpoints that
///   reference it as their `previous_checkpoint_id`.
/// - `chain_root_map` maps each checkpoint id to its chain root id
///   (`chainRootId ?? id`).
/// - `chain_groups` maps each chain root id to its member ids.
/// - `delta_ids` marks delta checkpoints: only surviving deltas seed
///   protection walks, because a surviving FULL restores from its own row
///   and needs no ancestor.
/// - `heads` marks restore chain heads (FULL with no base link): walks stop
///   after including the head, mirroring the delta restorer, so an older
///   retired generation is never kept alive by a newer one that merely
///   follows it in time.
#[derive(Debug, Clone, Default)]
pub struct CheckpointDependencyGraph {
    pub referenced_by: HashMap<String, Vec<String>>,
    pub chain_root_map: HashMap<String, String>,
    pub chain_groups: HashMap<String, Vec<String>>,
    heads: HashSet<String>,
    delta_ids: HashSet<String>,
}

impl CheckpointDependencyGraph {
    pub fn build(checkpoints: &[CheckpointStorageMetadata]) -> Self {
        let mut referenced_by: HashMap<String, Vec<String>> = HashMap::new();
        let mut chain_root_map: HashMap<String, String> = HashMap::new();
        let mut chain_groups: HashMap<String, Vec<String>> = HashMap::new();
        let mut heads: HashSet<String> = HashSet::new();
        let mut delta_ids: HashSet<String> = HashSet::new();
        for cp in checkpoints {
            if let Some(prev) = &cp.previous_checkpoint_id {
                if prev != &cp.id {
                    referenced_by
                        .entry(prev.clone())
                        .or_default()
                        .push(cp.id.clone());
                }
            }
            let root = cp.chain_root_id.clone().unwrap_or_else(|| cp.id.clone());
            chain_root_map.insert(cp.id.clone(), root.clone());
            chain_groups.entry(root).or_default().push(cp.id.clone());
            if cp.checkpoint_type == CheckpointType::Delta {
                delta_ids.insert(cp.id.clone());
            } else if cp.base_checkpoint_id.is_none() {
                heads.insert(cp.id.clone());
            }
        }
        Self {
            referenced_by,
            chain_root_map,
            chain_groups,
            heads,
            delta_ids,
        }
    }

    fn previous_map(&self) -> HashMap<String, String> {
        let mut previous_map: HashMap<String, String> = HashMap::new();
        for (prev, refs) in &self.referenced_by {
            for reference in refs {
                previous_map.insert(reference.clone(), prev.clone());
            }
        }
        previous_map
    }

    /// Walk the restore chain of one surviving delta: previous links back to
    /// and including the nearest chain head. A FULL head carries its own
    /// snapshot, so nothing older is needed to restore the seed.
    fn restore_chain(&self, seed_id: &str, previous_map: &HashMap<String, String>) -> Vec<String> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut current = previous_map.get(seed_id).cloned();
        while let Some(id) = current {
            if !visited.insert(id.clone()) {
                break;
            }
            chain.push(id.clone());
            if self.heads.contains(&id) {
                break;
            }
            current = previous_map.get(&id).cloned();
        }
        chain
    }

    /// Compute the set of candidate checkpoints that must be kept because a
    /// surviving delta needs them to restore: each surviving delta keeps the
    /// candidates on its restore chain (up to and including its FULL head).
    /// Candidates older than every surviving delta's head are free to retire,
    /// which is what lets count/age retention bound FULL generations.
    pub fn compute_protected(
        &self,
        candidate_ids: &HashSet<String>,
        all_checkpoint_ids: &HashSet<String>,
    ) -> HashSet<String> {
        let previous_map = self.previous_map();

        let mut protected = HashSet::new();
        let surviving_deltas: Vec<String> = all_checkpoint_ids
            .iter()
            .filter(|id| !candidate_ids.contains(*id) && self.delta_ids.contains(*id))
            .cloned()
            .collect();

        for seed in surviving_deltas {
            for id in self.restore_chain(&seed, &previous_map) {
                if candidate_ids.contains(&id) {
                    protected.insert(id);
                }
            }
        }

        protected
    }

    /// Protect chain members whose FULL baseline survives: when a candidate
    /// delta's chain root (the FULL anchor) is not a deletion candidate, the
    /// delta chain cannot be broken without losing the baseline, so every
    /// candidate member of that chain group is protected.
    pub fn chain_group_protected(&self, candidate_ids: &HashSet<String>) -> HashSet<String> {
        let mut protected = HashSet::new();
        for id in candidate_ids {
            let Some(root) = self.chain_root_map.get(id) else {
                continue;
            };
            if root == id {
                continue;
            }
            if !candidate_ids.contains(root) {
                protected.insert(id.clone());
            }
        }
        protected
    }

    /// Bases that must be kept because a surviving delta depends on them
    /// directly or through its restore chain. This is the base-priority
    /// rule complementing `compute_protected`: it runs on the finalized
    /// removal set (after dependency and chain-group protection already
    /// narrowed the candidates), so a checkpoint kept by those rules also
    /// keeps its base here. Walks seed from surviving deltas only and stop
    /// at the nearest chain head, matching the restorer. `removing` is the
    /// finalized removal set (already excluding protected/surviving
    /// checkpoints); the returned set are candidate bases that must be
    /// removed from `removing`. This is intentionally conservative (any
    /// candidate on a survivor's restore chain is kept); it does not
    /// implement generation-atomic retirement where a whole chain generation
    /// retires only together.
    pub fn bases_with_surviving_dependents(&self, removing: &HashSet<String>) -> HashSet<String> {
        let previous_map = self.previous_map();

        let mut protected = HashSet::new();
        for id in self.chain_root_map.keys() {
            if removing.contains(id) || !self.delta_ids.contains(id) {
                continue;
            }
            for prev in self.restore_chain(id, &previous_map) {
                if removing.contains(&prev) {
                    protected.insert(prev);
                }
            }
        }
        protected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::checkpoint::CheckpointStatus;
    use wf_types::checkpoint::CheckpointType;

    fn make_checkpoint(
        id: &str,
        checkpoint_type: CheckpointType,
        previous: Option<&str>,
        timestamp: i64,
    ) -> CheckpointStorageMetadata {
        CheckpointStorageMetadata {
            id: id.to_string(),
            entity_type: "test".to_string(),
            entity_id: "entity-1".to_string(),
            parent_entity_id: None,
            checkpoint_type,
            timestamp,
            status: CheckpointStatus::Completed,
            previous_checkpoint_id: previous.map(String::from),
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size: None,
            tags: None,
            custom_fields: None,
        }
    }

    #[test]
    fn build_graph_records_references() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, None, 1000),
            make_checkpoint("delta-1", CheckpointType::Delta, Some("full-1"), 2000),
            make_checkpoint("delta-2", CheckpointType::Delta, Some("delta-1"), 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        assert_eq!(
            graph.referenced_by.get("full-1"),
            Some(&vec!["delta-1".to_string()])
        );
        assert_eq!(
            graph.referenced_by.get("delta-1"),
            Some(&vec!["delta-2".to_string()])
        );
    }

    #[test]
    fn middle_delta_referenced_by_survivor_is_protected() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, None, 1000),
            make_checkpoint("delta-1", CheckpointType::Delta, Some("full-1"), 2000),
            make_checkpoint("delta-2", CheckpointType::Delta, Some("delta-1"), 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        let all: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> = ["full-1".to_string(), "delta-1".to_string()]
            .into_iter()
            .collect();

        let protected = graph.compute_protected(&candidates, &all);
        assert!(protected.contains("full-1"));
        assert!(protected.contains("delta-1"));
    }

    #[test]
    fn unreferenced_candidate_is_not_protected() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, None, 1000),
            make_checkpoint("full-2", CheckpointType::Full, None, 2000),
            make_checkpoint("delta-1", CheckpointType::Delta, Some("full-2"), 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        let all: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> = ["full-1".to_string()].into_iter().collect();

        let protected = graph.compute_protected(&candidates, &all);
        assert!(!protected.contains("full-1"));
    }

    #[test]
    fn chain_group_maps_and_protection() {
        let cp = |id: &str, ty: CheckpointType, prev: Option<&str>, root: Option<&str>| {
            let mut c = make_checkpoint(id, ty, prev, 1000);
            c.chain_root_id = root.map(String::from);
            c
        };
        let checkpoints = vec![
            cp("full-1", CheckpointType::Full, None, None),
            cp(
                "delta-1",
                CheckpointType::Delta,
                Some("full-1"),
                Some("full-1"),
            ),
            cp(
                "delta-2",
                CheckpointType::Delta,
                Some("delta-1"),
                Some("full-1"),
            ),
            cp("full-2", CheckpointType::Full, None, None),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);

        assert_eq!(
            graph.chain_root_map.get("delta-2"),
            Some(&"full-1".to_string())
        );
        let group = graph.chain_groups.get("full-1").unwrap();
        assert!(group.contains(&"delta-1".to_string()));
        assert!(group.contains(&"delta-2".to_string()));

        // delta-2 is a candidate, its root full-1 survives -> protected.
        let candidates: HashSet<String> = ["delta-2".to_string()].into_iter().collect();
        let protected = graph.chain_group_protected(&candidates);
        assert!(protected.contains("delta-2"));

        // A candidate whose own root is deleted is not protected.
        let candidates: HashSet<String> = ["full-1".to_string(), "delta-1".to_string()]
            .into_iter()
            .collect();
        let protected = graph.chain_group_protected(&candidates);
        assert!(!protected.contains("delta-1"), "root is also a candidate");
    }

    #[test]
    fn base_with_surviving_dependent_is_kept() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, None, 1000),
            make_checkpoint("delta-1", CheckpointType::Delta, Some("full-1"), 2000),
            make_checkpoint("full-2", CheckpointType::Full, None, 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        // full-1 and delta-1 are candidates; delta-1 is somehow kept (e.g. the
        // latest), so full-1 must also be kept.
        let removing: HashSet<String> = ["full-1".to_string(), "delta-1".to_string()]
            .into_iter()
            .collect();
        // Simulate delta-1 surviving: remove it from the removal set.
        let mut removing = removing;
        removing.remove("delta-1");
        let bases = graph.bases_with_surviving_dependents(&removing);
        assert!(bases.contains("full-1"), "full-1 is the base of a survivor");
    }

    #[test]
    fn base_without_surviving_dependent_may_retire() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, None, 1000),
            make_checkpoint("delta-1", CheckpointType::Delta, Some("full-1"), 2000),
            make_checkpoint("full-2", CheckpointType::Full, None, 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        // Whole chain retires together: nothing survives that depends on it.
        let removing: HashSet<String> = ["full-1".to_string(), "delta-1".to_string()]
            .into_iter()
            .collect();
        let bases = graph.bases_with_surviving_dependents(&removing);
        assert!(!bases.contains("full-1"));
    }

    #[test]
    fn compute_protected_terminates_on_cycle() {
        let checkpoints = vec![
            make_checkpoint("a", CheckpointType::Delta, Some("b"), 1000),
            make_checkpoint("b", CheckpointType::Delta, Some("a"), 2000),
            make_checkpoint("c", CheckpointType::Full, None, 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        let all: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> = ["a".to_string()].into_iter().collect();
        let protected = graph.compute_protected(&candidates, &all);
        assert!(protected.contains("a"));
    }

    #[test]
    fn full_generations_retire_without_delta_dependents() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, Some("full-0"), 1000),
            make_checkpoint("full-2", CheckpointType::Full, Some("full-1"), 2000),
            make_checkpoint("full-3", CheckpointType::Full, Some("full-2"), 3000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        let all: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> = ["full-1".to_string()].into_iter().collect();
        let protected = graph.compute_protected(&candidates, &all);
        assert!(
            protected.is_empty(),
            "a surviving FULL restores alone; older FULL generations may retire"
        );
    }

    #[test]
    fn restore_chain_stops_at_nearest_head() {
        let checkpoints = vec![
            make_checkpoint("full-1", CheckpointType::Full, None, 1000),
            make_checkpoint("delta-1", CheckpointType::Delta, Some("full-1"), 2000),
            make_checkpoint("full-2", CheckpointType::Full, Some("delta-1"), 3000),
            make_checkpoint("delta-2", CheckpointType::Delta, Some("full-2"), 4000),
        ];
        let graph = CheckpointDependencyGraph::build(&checkpoints);
        let all: HashSet<String> = checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> = ["full-1".to_string(), "delta-1".to_string()]
            .into_iter()
            .collect();
        let protected = graph.compute_protected(&candidates, &all);
        assert!(
            protected.is_empty(),
            "delta-2 restores from full-2; the older generation is unneeded"
        );
    }
}
