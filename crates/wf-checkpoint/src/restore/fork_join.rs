use super::integrity::ExecutionRegistry;
use std::collections::HashSet;
use wf_types::execution::{ExecutionHierarchy, ExecutionStatus};

#[derive(Debug, Clone, PartialEq)]
pub enum ForkPathStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct JoinStateInference {
    pub completed_paths: HashSet<String>,
    pub pending_paths: HashSet<String>,
    pub failed_paths: HashSet<String>,
}

impl JoinStateInference {
    pub fn is_complete(&self) -> bool {
        !self.completed_paths.is_empty() && self.pending_paths.is_empty()
    }

    pub fn all_paths(&self) -> HashSet<String> {
        self.completed_paths
            .union(&self.pending_paths)
            .cloned()
            .collect::<HashSet<_>>()
            .union(&self.failed_paths)
            .cloned()
            .collect()
    }

    pub fn path_status(&self, fork_path_id: &str) -> ForkPathStatus {
        if self.completed_paths.contains(fork_path_id) {
            ForkPathStatus::Completed
        } else if self.failed_paths.contains(fork_path_id) {
            ForkPathStatus::Failed
        } else {
            ForkPathStatus::Pending
        }
    }
}

pub struct ForkJoinStateInference;

impl ForkJoinStateInference {
    /// Infer FORK/JOIN completion status from the restored child executions
    /// registered in the registry.
    ///
    /// For each configured fork path id:
    /// - a child execution registered with that path id is classified by its
    ///   status (COMPLETED / FAILED or CANCELLED / otherwise pending);
    /// - a path that only exists in the snapshot hierarchy (child not yet
    ///   restored into the registry) is reported as pending.
    pub fn infer(
        fork_path_ids: &[String],
        parent_execution_id: &str,
        hierarchy: Option<&ExecutionHierarchy>,
        registry: &dyn ExecutionRegistry,
    ) -> JoinStateInference {
        let mut inference = JoinStateInference::default();

        for path_id in fork_path_ids {
            let child_id = registry.find_by_fork_path(parent_execution_id, path_id);

            match child_id.and_then(|id| registry.status_of(&id)) {
                Some(ExecutionStatus::Completed) => {
                    inference.completed_paths.insert(path_id.clone());
                }
                Some(ExecutionStatus::Failed) | Some(ExecutionStatus::Cancelled) => {
                    inference.failed_paths.insert(path_id.clone());
                }
                Some(_) => {
                    inference.pending_paths.insert(path_id.clone());
                }
                None => {
                    // Not restored into the registry yet: pending when the
                    // snapshot hierarchy still references this path, otherwise
                    // the path is no longer part of the execution.
                    let in_snapshot = hierarchy
                        .and_then(|h| h.children.as_ref())
                        .map(|children| {
                            children
                                .iter()
                                .any(|c| c.fork_path_id.as_deref() == Some(path_id.as_str()))
                        })
                        .unwrap_or(false);
                    if in_snapshot {
                        inference.pending_paths.insert(path_id.clone());
                    }
                }
            }
        }

        inference
    }

    /// Build the pathStatuses record from an inference, aligned with the
    /// `forkJoinAggregationState.pathStatuses` shape.
    pub fn path_statuses(&self, inference: &JoinStateInference) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        let all: Vec<String> = inference
            .completed_paths
            .iter()
            .chain(inference.pending_paths.iter())
            .chain(inference.failed_paths.iter())
            .cloned()
            .collect();
        for path in all {
            let status = match inference.path_status(&path) {
                ForkPathStatus::Pending => "PENDING",
                ForkPathStatus::Completed => "COMPLETED",
                ForkPathStatus::Failed => "FAILED",
            };
            map.insert(path, serde_json::Value::String(status.to_string()));
        }
        serde_json::Value::Object(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore::integrity::InMemoryExecutionRegistry;
    use wf_types::execution::{ChildExecutionReference, ExecutionType};

    fn make_hierarchy(children: Vec<(&str, &str)>) -> ExecutionHierarchy {
        ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "exec-1".to_string(),
            parent_execution_id: None,
            depth: 0,
            root_execution_id: None,
            children: Some(
                children
                    .into_iter()
                    .map(|(id, path)| ChildExecutionReference {
                        child_type: ExecutionType::Workflow,
                        child_id: id.to_string(),
                        created_at: 0,
                        fork_path_id: Some(path.to_string()),
                    })
                    .collect(),
            ),
        }
    }

    #[test]
    fn infers_completed_pending_and_failed_paths() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register_fork_path("child-1", ExecutionStatus::Completed, "exec-1", "path-1");
        registry.register_fork_path("child-2", ExecutionStatus::Running, "exec-1", "path-2");
        registry.register_fork_path("child-3", ExecutionStatus::Failed, "exec-1", "path-3");

        let paths = vec![
            "path-1".to_string(),
            "path-2".to_string(),
            "path-3".to_string(),
        ];
        let inference = ForkJoinStateInference::infer(&paths, "exec-1", None, &registry);

        assert!(inference.completed_paths.contains("path-1"));
        assert!(inference.pending_paths.contains("path-2"));
        assert!(inference.failed_paths.contains("path-3"));
        assert!(!inference.is_complete());
    }

    #[test]
    fn missing_registry_entry_is_pending() {
        let registry = InMemoryExecutionRegistry::new();
        let hierarchy = make_hierarchy(vec![("child-1", "path-1")]);

        let paths = vec!["path-1".to_string(), "path-2".to_string()];
        let inference =
            ForkJoinStateInference::infer(&paths, "exec-1", Some(&hierarchy), &registry);

        assert!(
            inference.pending_paths.contains("path-1"),
            "path referenced by the snapshot hierarchy is pending"
        );
        assert!(
            !inference.pending_paths.contains("path-2"),
            "path absent from both registry and hierarchy is not reported"
        );
        assert!(inference.completed_paths.is_empty());
    }

    #[test]
    fn complete_when_all_paths_completed() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register_fork_path("child-1", ExecutionStatus::Completed, "exec-1", "path-1");
        registry.register_fork_path("child-2", ExecutionStatus::Completed, "exec-1", "path-2");

        let paths = vec!["path-1".to_string(), "path-2".to_string()];
        let inference = ForkJoinStateInference::infer(&paths, "exec-1", None, &registry);

        assert!(inference.is_complete());
        assert_eq!(inference.all_paths().len(), 2);
    }

    #[test]
    fn cancelled_counts_as_failed() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register_fork_path("child-1", ExecutionStatus::Cancelled, "exec-1", "path-1");

        let paths = vec!["path-1".to_string()];
        let inference = ForkJoinStateInference::infer(&paths, "exec-1", None, &registry);

        assert!(inference.failed_paths.contains("path-1"));
        assert_eq!(inference.path_status("path-1"), ForkPathStatus::Failed);
    }

    #[test]
    fn path_statuses_produces_record() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register_fork_path("child-1", ExecutionStatus::Completed, "exec-1", "path-1");

        let paths = vec!["path-1".to_string()];
        let inference = ForkJoinStateInference::infer(&paths, "exec-1", None, &registry);
        let value = ForkJoinStateInference::path_statuses(&ForkJoinStateInference, &inference);

        assert_eq!(value["path-1"], serde_json::json!("COMPLETED"));
    }
}
