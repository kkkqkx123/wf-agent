use dashmap::DashMap;
use std::sync::Arc;
use wf_types::execution::{ExecutionHierarchy, ExecutionStatus};

#[derive(Debug, Clone)]
pub struct HierarchyValidationResult {
    pub valid: bool,
    pub issues: Vec<String>,
}

pub trait ExecutionRegistry: Send + Sync {
    fn register(&self, execution_id: &str, status: ExecutionStatus);
    fn register_with_parent(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        parent_execution_id: Option<&str>,
    );
    /// Register a child execution with its fork path id, enabling FORK/JOIN
    /// status inference from the registry.
    fn register_fork_path(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        parent_execution_id: &str,
        fork_path_id: &str,
    ) {
        let _ = fork_path_id;
        self.register_with_parent(execution_id, status, Some(parent_execution_id));
    }
    fn has(&self, execution_id: &str) -> bool;
    fn status_of(&self, execution_id: &str) -> Option<ExecutionStatus>;
    /// Find the child execution id registered under `parent_execution_id`
    /// with the given fork path id.
    fn find_by_fork_path(&self, parent_execution_id: &str, fork_path_id: &str) -> Option<String>;
}

#[derive(Clone)]
struct RegistryEntry {
    status: ExecutionStatus,
    parent_execution_id: Option<String>,
    fork_path_id: Option<String>,
}

pub struct InMemoryExecutionRegistry {
    entries: DashMap<String, RegistryEntry>,
}

impl InMemoryExecutionRegistry {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    pub fn register_fork_path(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        parent_execution_id: &str,
        fork_path_id: &str,
    ) {
        self.entries.insert(
            execution_id.to_string(),
            RegistryEntry {
                status,
                parent_execution_id: Some(parent_execution_id.to_string()),
                fork_path_id: Some(fork_path_id.to_string()),
            },
        );
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for InMemoryExecutionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionRegistry for InMemoryExecutionRegistry {
    fn register(&self, execution_id: &str, status: ExecutionStatus) {
        self.register_with_parent(execution_id, status, None);
    }

    fn register_with_parent(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        parent_execution_id: Option<&str>,
    ) {
        self.entries.insert(
            execution_id.to_string(),
            RegistryEntry {
                status,
                parent_execution_id: parent_execution_id.map(|p| p.to_string()),
                fork_path_id: None,
            },
        );
    }

    fn has(&self, execution_id: &str) -> bool {
        self.entries.contains_key(execution_id)
    }

    fn status_of(&self, execution_id: &str) -> Option<ExecutionStatus> {
        self.entries.get(execution_id).map(|e| e.status.clone())
    }

    fn find_by_fork_path(&self, parent_execution_id: &str, fork_path_id: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|entry| {
                entry.parent_execution_id.as_deref() == Some(parent_execution_id)
                    && entry.fork_path_id.as_deref() == Some(fork_path_id)
            })
            .map(|entry| entry.key().clone())
    }
}

pub struct HierarchyIntegrityService;

impl HierarchyIntegrityService {
    /// Validate hierarchy integrity against the registry:
    /// 1. Parent reference exists in the registry (if present).
    /// 2. All child references exist in the registry.
    /// 3. No orphaned references.
    pub fn validate_integrity(
        hierarchy: &ExecutionHierarchy,
        registry: &dyn ExecutionRegistry,
    ) -> HierarchyValidationResult {
        let mut issues = Vec::new();

        if let Some(parent_id) = &hierarchy.parent_execution_id {
            if !registry.has(parent_id) {
                issues.push(format!("Parent {} not found in registry", parent_id));
            }
        }

        if let Some(children) = &hierarchy.children {
            for child in children {
                if !registry.has(&child.child_id) {
                    issues.push(format!(
                        "Child {} ({:?}) not found in registry",
                        child.child_id, child.child_type
                    ));
                }
            }
        }

        HierarchyValidationResult {
            valid: issues.is_empty(),
            issues,
        }
    }

    /// Remove references to entities that no longer exist in the registry,
    /// producing a cleaned hierarchy.
    pub fn cleanup_orphaned_references(
        hierarchy: &ExecutionHierarchy,
        registry: &dyn ExecutionRegistry,
    ) -> ExecutionHierarchy {
        let mut cleaned = hierarchy.clone();
        if let Some(parent_id) = &cleaned.parent_execution_id {
            if !registry.has(parent_id) {
                cleaned.parent_execution_id = None;
            }
        }
        if let Some(children) = &cleaned.children {
            cleaned.children = Some(
                children
                    .iter()
                    .filter(|child| registry.has(&child.child_id))
                    .cloned()
                    .collect(),
            );
        }
        cleaned
    }
}

pub fn registry_from_restored_entities(
    restored: &[(String, ExecutionStatus, Option<String>)],
) -> Arc<InMemoryExecutionRegistry> {
    let registry = Arc::new(InMemoryExecutionRegistry::new());
    for (execution_id, status, parent) in restored {
        registry.register_with_parent(execution_id, status.clone(), parent.as_deref());
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::execution::{ChildExecutionReference, ExecutionType};

    fn make_hierarchy(
        parent: Option<&str>,
        children: Vec<(&str, ExecutionType)>,
    ) -> ExecutionHierarchy {
        ExecutionHierarchy {
            workflow_id: "wf-1".to_string(),
            execution_id: "exec-1".to_string(),
            parent_execution_id: parent.map(|p| p.to_string()),
            depth: 0,
            root_execution_id: None,
            ancestors: None,
            children: Some(
                children
                    .into_iter()
                    .map(|(id, t)| ChildExecutionReference {
                        child_type: t,
                        child_id: id.to_string(),
                        created_at: 0,
                        fork_path_id: None,
                    })
                    .collect(),
            ),
        }
    }

    #[test]
    fn validate_accepts_complete_hierarchy() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register("parent-1", ExecutionStatus::Running);
        registry.register("child-1", ExecutionStatus::Completed);
        registry.register_with_parent("child-1", ExecutionStatus::Completed, Some("parent-1"));

        let hierarchy =
            make_hierarchy(Some("parent-1"), vec![("child-1", ExecutionType::Workflow)]);
        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(result.valid);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn validate_reports_missing_parent_and_children() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register("child-2", ExecutionStatus::Running);

        let hierarchy = make_hierarchy(
            Some("missing-parent"),
            vec![
                ("child-1", ExecutionType::Workflow),
                ("child-2", ExecutionType::AgentLoop),
            ],
        );
        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert_eq!(result.issues.len(), 2);
    }

    #[test]
    fn cleanup_removes_orphaned_references() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register("parent-1", ExecutionStatus::Running);
        registry.register("child-1", ExecutionStatus::Completed);

        let hierarchy = make_hierarchy(
            Some("missing-parent"),
            vec![
                ("child-1", ExecutionType::Workflow),
                ("child-2", ExecutionType::Workflow),
            ],
        );
        let cleaned = HierarchyIntegrityService::cleanup_orphaned_references(&hierarchy, &registry);

        assert!(cleaned.parent_execution_id.is_none());
        let children = cleaned.children.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].child_id, "child-1");
    }

    #[test]
    fn registry_tracks_parent_relationship() {
        let registry = InMemoryExecutionRegistry::new();
        registry.register_with_parent("child-1", ExecutionStatus::Completed, Some("parent-1"));

        assert!(registry.has("child-1"));
        assert_eq!(
            registry.status_of("child-1"),
            Some(ExecutionStatus::Completed)
        );
        assert!(!registry.has("unknown"));
        assert_eq!(registry.status_of("unknown"), None);
    }
}
