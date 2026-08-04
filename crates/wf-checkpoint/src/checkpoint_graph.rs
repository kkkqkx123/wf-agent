use std::collections::{HashMap, HashSet};

use wf_types::storage::CheckpointStorageMetadata;

/// Dependency graph over an entity's checkpoint chain, aligned with the TS
/// `checkpoint-graph.ts` module.
///
/// - `referenced_by` maps each checkpoint id to the ids of checkpoints that
///   reference it as their `previous_checkpoint_id`.
/// - `chain_root_map` maps each checkpoint id to its chain root id
///   (`chainRootId ?? id`).
/// - `chain_groups` maps each chain root id to its member ids.
#[derive(Debug, Clone, Default)]
pub struct CheckpointDependencyGraph {
    pub referenced_by: HashMap<String, Vec<String>>,
    pub chain_root_map: HashMap<String, String>,
    pub chain_groups: HashMap<String, Vec<String>>,
}

impl CheckpointDependencyGraph {
    pub fn build(checkpoints: &[CheckpointStorageMetadata]) -> Self {
        let mut referenced_by: HashMap<String, Vec<String>> = HashMap::new();
        let mut chain_root_map: HashMap<String, String> = HashMap::new();
        let mut chain_groups: HashMap<String, Vec<String>> = HashMap::new();
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
        }
        Self {
            referenced_by,
            chain_root_map,
            chain_groups,
        }
    }

    /// Compute the set of candidate checkpoints that must be kept because a
    /// surviving checkpoint depends on them through the `previous_checkpoint_id`
    /// chain. Aligned with TS `computeProtectedCheckpoints`.
    pub fn compute_protected(
        &self,
        candidate_ids: &HashSet<String>,
        all_checkpoint_ids: &HashSet<String>,
    ) -> HashSet<String> {
        let mut previous_map: HashMap<String, String> = HashMap::new();
        for (prev, refs) in &self.referenced_by {
            for reference in refs {
                previous_map.insert(reference.clone(), prev.clone());
            }
        }

        let mut protected = HashSet::new();
        let surviving_ids: Vec<String> = all_checkpoint_ids
            .iter()
            .filter(|id| !candidate_ids.contains(*id))
            .cloned()
            .collect();

        for surviving_id in surviving_ids {
            let mut current = Some(surviving_id);
            while let Some(id) = current {
                if candidate_ids.contains(&id) {
                    protected.insert(id.clone());
                }
                current = previous_map.get(&id).cloned();
            }
        }

        protected
    }

    /// Protect chain members whose FULL baseline survives: when a candidate
    /// delta's chain root (the FULL anchor) is not a deletion candidate, the
    /// delta chain cannot be broken without losing the baseline, so every
    /// candidate member of that chain group is protected. This mirrors the
    /// TS size-based strategy's chain-group check.
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
}
