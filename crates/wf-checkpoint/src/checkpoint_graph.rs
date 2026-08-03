use std::collections::{HashMap, HashSet};

use wf_types::storage::CheckpointStorageMetadata;

/// Dependency graph over an entity's checkpoint chain, aligned with the TS
/// `checkpoint-graph.ts` module.
///
/// `referenced_by` maps each checkpoint id to the ids of checkpoints that
/// reference it as their `previous_checkpoint_id`.
#[derive(Debug, Clone, Default)]
pub struct CheckpointDependencyGraph {
    pub referenced_by: HashMap<String, Vec<String>>,
}

impl CheckpointDependencyGraph {
    pub fn build(checkpoints: &[CheckpointStorageMetadata]) -> Self {
        let mut referenced_by: HashMap<String, Vec<String>> = HashMap::new();
        for cp in checkpoints {
            if let Some(prev) = &cp.previous_checkpoint_id {
                if prev != &cp.id {
                    referenced_by
                        .entry(prev.clone())
                        .or_default()
                        .push(cp.id.clone());
                }
            }
        }
        Self { referenced_by }
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
        let all: HashSet<String> =
            checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> =
            ["full-1".to_string(), "delta-1".to_string()].into_iter().collect();

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
        let all: HashSet<String> =
            checkpoints.iter().map(|c| c.id.clone()).collect();
        let candidates: HashSet<String> = ["full-1".to_string()].into_iter().collect();

        let protected = graph.compute_protected(&candidates, &all);
        assert!(!protected.contains("full-1"));
    }
}
