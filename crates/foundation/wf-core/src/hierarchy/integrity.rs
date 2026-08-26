use wf_types::execution::{ChildExecutionReference, ExecutionType};
use wf_types::Id;

use super::manager::{ExecutionHierarchyMetadata, ParentExecutionContext, MAX_DEPTH};

#[derive(Debug, Clone)]
pub struct HierarchyValidationResult {
    pub valid: bool,
    pub issues: Vec<String>,
}

pub trait HierarchyEntityProvider {
    fn execution_id(&self) -> &Id;
    fn parent_context(&self) -> Option<ParentExecutionContext>;
    fn root_execution_id(&self) -> Id;
    fn root_execution_type(&self) -> ExecutionType;
    fn depth(&self) -> u32;
}

pub trait HierarchyRegistry: Send + Sync {
    fn contains(&self, execution_id: &Id) -> bool;

    fn get_entity(&self, execution_id: &Id) -> Option<Box<dyn HierarchyEntityProvider>>;
}

pub struct HierarchyIntegrityService;

impl HierarchyIntegrityService {
    pub fn validate_integrity(
        hierarchy: &ExecutionHierarchyMetadata,
        registry: &dyn HierarchyRegistry,
    ) -> HierarchyValidationResult {
        let mut issues: Vec<String> = Vec::new();

        if let Some(ref parent) = hierarchy.parent {
            if !registry.contains(&parent.parent_id) {
                issues.push(format!("Parent execution '{}' not found", parent.parent_id));
            }
        }

        for child in &hierarchy.children {
            if !registry.contains(&child.child_id) {
                issues.push(format!("Child execution '{}' not found", child.child_id));
            }
        }

        if !registry.contains(&hierarchy.root_execution_id) {
            issues.push(format!(
                "Root execution '{}' not found",
                hierarchy.root_execution_id
            ));
        }

        if let Some(ancestors) = &hierarchy.ancestors {
            for ancestor_id in ancestors {
                if !registry.contains(ancestor_id) {
                    issues.push(format!("Ancestor execution '{ancestor_id}' not found"));
                }
            }

            if let Some(head) = ancestors.first() {
                if *head != hierarchy.root_execution_id {
                    issues.push(format!(
                        "Ancestor chain starts at '{}' which differs from root execution '{}'",
                        head, hierarchy.root_execution_id
                    ));
                }
            }

            match (&hierarchy.parent, ancestors.last()) {
                (Some(parent), Some(tail)) => {
                    if *tail != parent.parent_id {
                        issues.push(format!(
                            "Ancestor chain ends at '{}' which differs from parent execution '{}'",
                            tail, parent.parent_id
                        ));
                    }
                }
                (Some(parent), None) => {
                    issues.push(format!(
                        "Parent execution '{}' set but ancestor chain is empty",
                        parent.parent_id
                    ));
                }
                _ => {}
            }

            if ancestors.len() >= MAX_DEPTH as usize {
                issues.push(format!(
                    "Ancestor chain length {} exceeds maximum allowed depth {}",
                    ancestors.len(),
                    MAX_DEPTH
                ));
            }
        }

        if hierarchy.depth > MAX_DEPTH {
            issues.push(format!(
                "Hierarchy depth {} exceeds maximum allowed depth {}",
                hierarchy.depth, MAX_DEPTH
            ));
        }

        HierarchyValidationResult {
            valid: issues.is_empty(),
            issues,
        }
    }

    pub fn cleanup_orphaned_references(
        hierarchy: &ExecutionHierarchyMetadata,
        registry: &dyn HierarchyRegistry,
    ) -> ExecutionHierarchyMetadata {
        let parent = hierarchy.parent.as_ref().and_then(|p| {
            if registry.contains(&p.parent_id) {
                Some(p.clone())
            } else {
                None
            }
        });

        let children: Vec<ChildExecutionReference> = hierarchy
            .children
            .iter()
            .filter(|c| registry.contains(&c.child_id))
            .cloned()
            .collect();

        // A chain is only meaningful while its tail (the direct parent)
        // survives: when the parent link is orphaned the whole chain is
        // unverifiable and gets dropped. Otherwise stale ancestor ids are
        // filtered individually.
        let ancestors = if parent.is_none() && hierarchy.parent.is_some() {
            None
        } else {
            hierarchy.ancestors.as_ref().map(|chain| {
                chain
                    .iter()
                    .filter(|id| registry.contains(id))
                    .cloned()
                    .collect::<Vec<Id>>()
            })
        };
        let ancestors = ancestors.filter(|chain| !chain.is_empty());

        ExecutionHierarchyMetadata {
            parent,
            children,
            depth: hierarchy.depth,
            root_execution_id: hierarchy.root_execution_id.clone(),
            root_execution_type: hierarchy.root_execution_type.clone(),
            ancestors,
        }
    }

    pub fn repair_root_info(
        hierarchy: &ExecutionHierarchyMetadata,
        registry: &dyn HierarchyRegistry,
    ) -> ExecutionHierarchyMetadata {
        let mut repaired = hierarchy.clone();

        if let Some(ref parent_ctx) = hierarchy.parent {
            if let Some(entity) = registry.get_entity(&parent_ctx.parent_id) {
                repaired.root_execution_id = entity.root_execution_id();
                repaired.root_execution_type = entity.root_execution_type();
                // Keep the chain head aligned with the repaired root so the
                // "chain starts at root" invariant holds after repair.
                if let Some(chain) = repaired.ancestors.as_mut() {
                    if let Some(head) = chain.first_mut() {
                        *head = repaired.root_execution_id.clone();
                    }
                }
            }
        }

        repaired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEntity {
        id: Id,
        parent: Option<ParentExecutionContext>,
        root_id: Id,
        root_type: ExecutionType,
        depth: u32,
    }

    impl HierarchyEntityProvider for TestEntity {
        fn execution_id(&self) -> &Id {
            &self.id
        }
        fn parent_context(&self) -> Option<ParentExecutionContext> {
            self.parent.clone()
        }
        fn root_execution_id(&self) -> Id {
            self.root_id.clone()
        }
        fn root_execution_type(&self) -> ExecutionType {
            self.root_type.clone()
        }
        fn depth(&self) -> u32 {
            self.depth
        }
    }

    struct TestRegistry {
        entities: Vec<TestEntity>,
    }

    impl HierarchyRegistry for TestRegistry {
        fn contains(&self, execution_id: &Id) -> bool {
            self.entities.iter().any(|e| e.id == *execution_id)
        }

        fn get_entity(&self, execution_id: &Id) -> Option<Box<dyn HierarchyEntityProvider>> {
            self.entities
                .iter()
                .find(|e| e.id == *execution_id)
                .map(|e| {
                    let cloned = TestEntity {
                        id: e.id.clone(),
                        parent: e.parent.clone(),
                        root_id: e.root_id.clone(),
                        root_type: e.root_type.clone(),
                        depth: e.depth,
                    };
                    Box::new(cloned) as Box<dyn HierarchyEntityProvider>
                })
        }
    }

    fn make_id(s: &str) -> Id {
        s.to_string()
    }

    fn make_hierarchy(
        parent: Option<ParentExecutionContext>,
        children: Vec<ChildExecutionReference>,
        root_id: Id,
        root_type: ExecutionType,
    ) -> ExecutionHierarchyMetadata {
        let depth = if parent.is_some() { 1 } else { 0 };
        ExecutionHierarchyMetadata {
            parent,
            children,
            depth,
            root_execution_id: root_id,
            root_execution_type: root_type,
            ancestors: None,
        }
    }

    #[test]
    fn test_validate_integrity_valid() {
        let root_id = make_id("root-1");
        let _child_id = make_id("child-1");
        let registry = TestRegistry {
            entities: vec![TestEntity {
                id: root_id.clone(),
                parent: None,
                root_id: root_id.clone(),
                root_type: ExecutionType::Workflow,
                depth: 0,
            }],
        };

        let hierarchy = make_hierarchy(None, vec![], root_id, ExecutionType::Workflow);

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(result.valid);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_validate_integrity_parent_not_found() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let registry = TestRegistry { entities: vec![] };

        let hierarchy = make_hierarchy(
            Some(ParentExecutionContext {
                parent_id: parent_id.clone(),
                parent_type: ExecutionType::Workflow,
            }),
            vec![],
            root_id.clone(),
            ExecutionType::Workflow,
        );

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("Parent")));
        assert!(result.issues.iter().any(|i| i.contains("Root")));
    }

    #[test]
    fn test_validate_integrity_child_not_found() {
        let root_id = make_id("root-1");
        let child_id = make_id("child-1");
        let registry = TestRegistry {
            entities: vec![TestEntity {
                id: root_id.clone(),
                parent: None,
                root_id: root_id.clone(),
                root_type: ExecutionType::Workflow,
                depth: 0,
            }],
        };

        let hierarchy = make_hierarchy(
            None,
            vec![ChildExecutionReference {
                child_type: ExecutionType::AgentLoop,
                child_id: child_id.clone(),
                created_at: 0,
                fork_path_id: None,
            }],
            root_id,
            ExecutionType::Workflow,
        );

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("Child")));
    }

    #[test]
    fn test_validate_integrity_depth_exceeded() {
        let root_id = make_id("root-1");
        let registry = TestRegistry {
            entities: vec![TestEntity {
                id: root_id.clone(),
                parent: None,
                root_id: root_id.clone(),
                root_type: ExecutionType::Workflow,
                depth: 0,
            }],
        };

        let mut hierarchy = make_hierarchy(None, vec![], root_id, ExecutionType::Workflow);
        hierarchy.depth = MAX_DEPTH + 1;

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("depth")));
    }

    #[test]
    fn test_cleanup_orphaned_removes_missing_parent() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let registry = TestRegistry { entities: vec![] };

        let hierarchy = make_hierarchy(
            Some(ParentExecutionContext {
                parent_id: parent_id.clone(),
                parent_type: ExecutionType::Workflow,
            }),
            vec![],
            root_id.clone(),
            ExecutionType::Workflow,
        );

        let repaired =
            HierarchyIntegrityService::cleanup_orphaned_references(&hierarchy, &registry);
        assert!(repaired.parent.is_none());
    }

    #[test]
    fn test_cleanup_orphaned_removes_missing_children() {
        let root_id = make_id("root-1");
        let child_id = make_id("child-1");
        let registry = TestRegistry {
            entities: vec![TestEntity {
                id: root_id.clone(),
                parent: None,
                root_id: root_id.clone(),
                root_type: ExecutionType::Workflow,
                depth: 0,
            }],
        };

        let hierarchy = make_hierarchy(
            None,
            vec![ChildExecutionReference {
                child_type: ExecutionType::AgentLoop,
                child_id: child_id.clone(),
                created_at: 0,
                fork_path_id: None,
            }],
            root_id.clone(),
            ExecutionType::Workflow,
        );

        let repaired =
            HierarchyIntegrityService::cleanup_orphaned_references(&hierarchy, &registry);
        assert!(repaired.children.is_empty());
    }

    #[test]
    fn test_repair_root_info_from_parent() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let true_root_id = make_id("true-root-1");
        let registry = TestRegistry {
            entities: vec![TestEntity {
                id: parent_id.clone(),
                parent: None,
                root_id: true_root_id.clone(),
                root_type: ExecutionType::Workflow,
                depth: 0,
            }],
        };

        let hierarchy = make_hierarchy(
            Some(ParentExecutionContext {
                parent_id: parent_id.clone(),
                parent_type: ExecutionType::Workflow,
            }),
            vec![],
            root_id,
            ExecutionType::AgentLoop,
        );

        let repaired = HierarchyIntegrityService::repair_root_info(&hierarchy, &registry);
        assert_eq!(repaired.root_execution_id, true_root_id);
        assert_eq!(repaired.root_execution_type, ExecutionType::Workflow);
    }

    #[test]
    fn test_repair_root_info_no_parent() {
        let root_id = make_id("root-1");
        let registry = TestRegistry { entities: vec![] };

        let hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);

        let repaired = HierarchyIntegrityService::repair_root_info(&hierarchy, &registry);
        assert_eq!(repaired.root_execution_id, root_id);
        assert_eq!(repaired.root_execution_type, ExecutionType::Workflow);
    }

    #[test]
    fn test_repair_root_info_parent_not_found() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let registry = TestRegistry { entities: vec![] };

        let hierarchy = make_hierarchy(
            Some(ParentExecutionContext {
                parent_id,
                parent_type: ExecutionType::Workflow,
            }),
            vec![],
            root_id.clone(),
            ExecutionType::AgentLoop,
        );

        let repaired = HierarchyIntegrityService::repair_root_info(&hierarchy, &registry);
        assert_eq!(repaired.root_execution_id, root_id);
        assert_eq!(repaired.root_execution_type, ExecutionType::AgentLoop);
    }

    #[test]
    fn test_validate_accepts_consistent_ancestors() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let registry = TestRegistry {
            entities: vec![
                TestEntity {
                    id: root_id.clone(),
                    parent: None,
                    root_id: root_id.clone(),
                    root_type: ExecutionType::Workflow,
                    depth: 0,
                },
                TestEntity {
                    id: parent_id.clone(),
                    parent: None,
                    root_id: root_id.clone(),
                    root_type: ExecutionType::Workflow,
                    depth: 0,
                },
            ],
        };

        let mut hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);
        hierarchy.parent = Some(ParentExecutionContext {
            parent_id: parent_id.clone(),
            parent_type: ExecutionType::Workflow,
        });
        hierarchy.ancestors = Some(vec![root_id.clone(), parent_id.clone()]);

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(result.valid, "unexpected issues: {:?}", result.issues);
    }

    #[test]
    fn test_validate_reports_missing_ancestor() {
        let root_id = make_id("root-1");
        let ghost_id = make_id("ghost-1");
        let registry = TestRegistry { entities: vec![] };

        let mut hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);
        hierarchy.ancestors = Some(vec![root_id.clone(), ghost_id]);

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.contains("Ancestor") && i.contains("ghost-1")));
    }

    #[test]
    fn test_validate_reports_head_root_mismatch() {
        let root_id = make_id("root-1");
        let other_id = make_id("other-1");
        let registry = TestRegistry { entities: vec![] };

        let mut hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);
        hierarchy.ancestors = Some(vec![other_id]);

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.contains("differs from root")));
    }

    #[test]
    fn test_validate_reports_tail_parent_mismatch() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let stale_id = make_id("stale-1");
        let registry = TestRegistry { entities: vec![] };

        let mut hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);
        hierarchy.parent = Some(ParentExecutionContext {
            parent_id: parent_id.clone(),
            parent_type: ExecutionType::Workflow,
        });
        hierarchy.ancestors = Some(vec![root_id, stale_id]);

        let result = HierarchyIntegrityService::validate_integrity(&hierarchy, &registry);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.contains("differs from parent")));
    }

    #[test]
    fn test_cleanup_filters_stale_ancestors() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        let stale_id = make_id("stale-1");
        let registry = TestRegistry {
            entities: vec![
                TestEntity {
                    id: root_id.clone(),
                    parent: None,
                    root_id: root_id.clone(),
                    root_type: ExecutionType::Workflow,
                    depth: 0,
                },
                TestEntity {
                    id: parent_id.clone(),
                    parent: None,
                    root_id: root_id.clone(),
                    root_type: ExecutionType::Workflow,
                    depth: 0,
                },
            ],
        };

        let mut hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);
        hierarchy.parent = Some(ParentExecutionContext {
            parent_id: parent_id.clone(),
            parent_type: ExecutionType::Workflow,
        });
        hierarchy.ancestors = Some(vec![root_id.clone(), stale_id.clone(), parent_id.clone()]);

        let cleaned = HierarchyIntegrityService::cleanup_orphaned_references(&hierarchy, &registry);
        assert_eq!(
            cleaned.ancestors,
            Some(vec![root_id, parent_id]),
            "stale ancestor removed, live ones kept"
        );
    }

    #[test]
    fn test_cleanup_drops_chain_when_parent_orphaned() {
        let root_id = make_id("root-1");
        let parent_id = make_id("parent-1");
        // Registry does not contain the parent.
        let registry = TestRegistry { entities: vec![] };

        let mut hierarchy = make_hierarchy(None, vec![], root_id.clone(), ExecutionType::Workflow);
        hierarchy.parent = Some(ParentExecutionContext {
            parent_id: parent_id.clone(),
            parent_type: ExecutionType::Workflow,
        });
        hierarchy.ancestors = Some(vec![root_id, parent_id]);

        let cleaned = HierarchyIntegrityService::cleanup_orphaned_references(&hierarchy, &registry);
        assert!(cleaned.parent.is_none());
        assert!(cleaned.ancestors.is_none(), "chain dropped with its tail");
    }

    #[test]
    fn test_repair_root_info_aligns_chain_head() {
        let true_root_id = make_id("true-root-1");
        let parent_id = make_id("parent-1");
        let registry = TestRegistry {
            entities: vec![TestEntity {
                id: parent_id.clone(),
                parent: None,
                root_id: true_root_id.clone(),
                root_type: ExecutionType::Workflow,
                depth: 0,
            }],
        };

        let mut hierarchy = make_hierarchy(
            None,
            vec![],
            make_id("wrong-root"),
            ExecutionType::AgentLoop,
        );
        hierarchy.parent = Some(ParentExecutionContext {
            parent_id: parent_id.clone(),
            parent_type: ExecutionType::Workflow,
        });
        hierarchy.ancestors = Some(vec![make_id("wrong-root"), parent_id]);

        let repaired = HierarchyIntegrityService::repair_root_info(&hierarchy, &registry);
        assert_eq!(repaired.root_execution_id, true_root_id);
        assert_eq!(
            repaired.ancestors,
            Some(vec![true_root_id, make_id("parent-1")])
        );
    }
}
