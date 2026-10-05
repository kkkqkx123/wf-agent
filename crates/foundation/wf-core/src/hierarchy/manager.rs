use std::collections::HashMap;
use std::sync::RwLock;

use wf_common::lock::read_ok;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wf_types::execution::{ExecutionType, ForkPath};
use wf_types::Id;

use crate::error::{CoreError, CoreResult};

pub const MAX_DEPTH: u32 = 10;

/// One live child execution tracked by its parent's manager. This is runtime
/// bookkeeping for subtree control (pause / resume / stop / cancel) and depth
/// accounting; it is never persisted. Durable parent-child links live on the
/// child side, so a child's own record is the single source of truth for the
/// shape of the tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildExecutionReference {
    pub child_type: ExecutionType,
    pub child_id: Id,
    pub created_at: wf_types::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_path: Option<ForkPath>,
}

impl ChildExecutionReference {
    pub fn branch_path_id(&self) -> Option<&str> {
        self.fork_path.as_ref().map(|p| p.branch_path_id())
    }

    pub fn fork_node_id(&self) -> Option<&str> {
        self.fork_path.as_ref().map(|p| p.fork_node_id())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentExecutionContext {
    pub parent_id: Id,
    pub parent_type: ExecutionType,
}

pub struct ExecutionHierarchyManager {
    inner: RwLock<HierarchyInner>,
}

#[derive(Debug)]
struct HierarchyInner {
    execution_id: Id,
    execution_type: ExecutionType,
    parent: Option<ParentExecutionContext>,
    children: HashMap<String, ChildExecutionReference>,
    depth: u32,
    root_execution_id: Id,
    root_execution_type: ExecutionType,
    /// Root-to-parent execution id chain (oldest first, excluding self).
    /// Populated by `set_parent` when the full chain is known; empty for
    /// roots or when only the direct parent is known.
    ancestors: Vec<Id>,
    /// Set when this execution was produced by a FORK branch.
    fork_path: Option<ForkPath>,
}

impl ExecutionHierarchyManager {
    pub fn new(execution_id: Id, execution_type: ExecutionType) -> Self {
        Self {
            inner: RwLock::new(HierarchyInner {
                execution_id: execution_id.clone(),
                execution_type: execution_type.clone(),
                parent: None,
                children: HashMap::new(),
                depth: 0,
                root_execution_id: execution_id,
                root_execution_type: execution_type,
                ancestors: Vec::new(),
                fork_path: None,
            }),
        }
    }

    /// Link this execution under `parent`. When `parent_ancestors` is
    /// provided it is the parent's own root-to-parent chain; the parent id
    /// is appended to form this execution's chain so deep hierarchies carry
    /// full ancestry through `to_metadata`. `None` leaves any existing chain
    /// untouched (e.g. one set explicitly beforehand via `set_ancestors`).
    pub fn set_parent(
        &self,
        parent: ParentExecutionContext,
        parent_ancestors: Option<&[Id]>,
    ) -> CoreResult<()> {
        if parent.parent_id == wf_common::lock::read_ok(self.inner.read()).execution_id {
            return Err(CoreError::StateError(format!(
                "cannot set self ({}) as parent",
                parent.parent_id
            )));
        }

        let mut inner = wf_common::lock::write_ok(self.inner.write());
        let parent_id = parent.parent_id.clone();
        let parent_type = parent.parent_type.clone();
        inner.parent = Some(parent);
        if let Some(parent_ancestors) = parent_ancestors {
            let mut chain = parent_ancestors.to_vec();
            if chain.last() != Some(&parent_id) {
                chain.push(parent_id.clone());
            }
            inner.ancestors = chain;
        }
        let new_depth = if inner.ancestors.is_empty() {
            1
        } else {
            inner.ancestors.len() as u32
        };

        if new_depth > MAX_DEPTH {
            return Err(CoreError::HierarchyDepthExceeded {
                depth: new_depth,
                max_depth: MAX_DEPTH,
            });
        }

        inner.depth = new_depth;
        if inner.ancestors.is_empty() {
            inner.root_execution_id = parent_id.clone();
            inner.root_execution_type = match parent_type {
                ExecutionType::Workflow => ExecutionType::Workflow,
                ExecutionType::AgentLoop => ExecutionType::AgentLoop,
            };
        } else if let Some(root) = inner.ancestors.first().cloned() {
            inner.root_execution_id = root;
        }
        inner.recalculate();

        Ok(())
    }

    pub fn set_parent_full(
        &self,
        parent: ParentExecutionContext,
        parent_ancestors: &[Id],
        parent_depth: u32,
        parent_root_id: Id,
        parent_root_type: ExecutionType,
    ) -> CoreResult<()> {
        if parent.parent_id == wf_common::lock::read_ok(self.inner.read()).execution_id {
            return Err(CoreError::StateError(format!(
                "cannot set self ({}) as parent",
                parent.parent_id
            )));
        }
        let mut inner = wf_common::lock::write_ok(self.inner.write());
        let new_depth = parent_depth.saturating_add(1);
        if new_depth > MAX_DEPTH {
            return Err(CoreError::HierarchyDepthExceeded {
                depth: new_depth,
                max_depth: MAX_DEPTH,
            });
        }
        inner.parent = Some(parent.clone());
        let mut chain = parent_ancestors.to_vec();
        if chain.last() != Some(&parent.parent_id) {
            chain.push(parent.parent_id.clone());
        }
        inner.ancestors = chain;
        inner.depth = new_depth;
        inner.root_execution_id = parent_root_id;
        inner.root_execution_type = parent_root_type;
        Ok(())
    }

    pub fn execution_id(&self) -> Id {
        wf_common::lock::read_ok(self.inner.read())
            .execution_id
            .clone()
    }

    pub fn execution_type(&self) -> ExecutionType {
        wf_common::lock::read_ok(self.inner.read())
            .execution_type
            .clone()
    }

    pub fn parent(&self) -> Option<ParentExecutionContext> {
        wf_common::lock::read_ok(self.inner.read()).parent.clone()
    }

    pub fn parent_id(&self) -> Option<Id> {
        wf_common::lock::read_ok(self.inner.read())
            .parent
            .as_ref()
            .map(|p| p.parent_id.clone())
    }

    pub fn derive_child(
        self: &Arc<Self>,
        child_id: Id,
        child_type: ExecutionType,
        fork_path: Option<ForkPath>,
    ) -> CoreResult<Arc<Self>> {
        let (
            parent_id,
            parent_type,
            parent_ancestors,
            parent_depth,
            parent_root_id,
            parent_root_type,
        );
        {
            let inner = wf_common::lock::read_ok(self.inner.read());
            if inner.execution_id == child_id {
                return Err(CoreError::StateError(format!(
                    "cannot derive self ({}) as child",
                    child_id
                )));
            }
            parent_id = inner.execution_id.clone();
            parent_type = inner.execution_type.clone();
            parent_ancestors = inner.ancestors.clone();
            parent_depth = inner.depth;
            parent_root_id = inner.root_execution_id.clone();
            parent_root_type = inner.root_execution_type.clone();
        }
        let new_depth = parent_depth.saturating_add(1);
        if new_depth > MAX_DEPTH {
            return Err(CoreError::HierarchyDepthExceeded {
                depth: new_depth,
                max_depth: MAX_DEPTH,
            });
        }
        let mut chain = parent_ancestors;
        if chain.last() != Some(&parent_id) {
            chain.push(parent_id.clone());
        }
        let child = Arc::new(Self {
            inner: RwLock::new(HierarchyInner {
                execution_id: child_id.clone(),
                execution_type: child_type.clone(),
                parent: Some(ParentExecutionContext {
                    parent_id: parent_id.clone(),
                    parent_type,
                }),
                children: HashMap::new(),
                depth: new_depth,
                root_execution_id: parent_root_id,
                root_execution_type: parent_root_type,
                ancestors: chain,
                fork_path: fork_path.clone(),
            }),
        });
        let child_ref = ChildExecutionReference {
            child_type,
            child_id,
            created_at: wf_common::time::now(),
            fork_path,
        };
        self.register_child_ref(child_ref);
        Ok(child)
    }

    /// Rebuild a manager from a persisted record hierarchy. The parent type
    /// falls back to `default_parent_type` when the record predates it; the
    /// root falls back through explicit root, ancestor chain head, parent,
    /// then self. Children are not restored: they are discovered by querying
    /// the child records themselves, so a manager rebuilt from storage never
    /// carries a stale child list.
    pub fn restore(
        execution_id: Id,
        execution_type: ExecutionType,
        hierarchy: &wf_types::execution::ExecutionHierarchy,
        default_parent_type: ExecutionType,
    ) -> Arc<Self> {
        let manager = Arc::new(Self::new(execution_id, execution_type));
        let parent =
            hierarchy
                .parent_execution_id
                .clone()
                .map(|parent_id| ParentExecutionContext {
                    parent_type: hierarchy
                        .parent_execution_type
                        .clone()
                        .unwrap_or(default_parent_type),
                    parent_id,
                });
        let ancestors = hierarchy.ancestors.clone().unwrap_or_default();
        let root_id = hierarchy
            .root_execution_id
            .clone()
            .or_else(|| ancestors.first().cloned())
            .or_else(|| parent.as_ref().map(|p| p.parent_id.clone()))
            .unwrap_or_else(|| manager.execution_id());
        let root_type = hierarchy.root_execution_type.clone().unwrap_or_else(|| {
            if root_id == manager.execution_id() {
                manager.execution_type()
            } else {
                manager.root_execution_type()
            }
        });
        manager.sync_restored(
            parent,
            ancestors,
            hierarchy.depth,
            root_id,
            root_type,
            hierarchy.fork_path.clone(),
        );
        manager
    }

    pub fn register_child_ref(&self, child_ref: ChildExecutionReference) {
        self.add_child(child_ref);
    }

    pub fn sync_restored(
        &self,
        parent: Option<ParentExecutionContext>,
        ancestors: Vec<Id>,
        depth: u32,
        root_id: Id,
        root_type: ExecutionType,
        fork_path: Option<ForkPath>,
    ) {
        let mut inner = wf_common::lock::write_ok(self.inner.write());
        inner.parent = parent;
        inner.ancestors = ancestors;
        inner.depth = depth;
        inner.root_execution_id = root_id;
        inner.root_execution_type = root_type;
        inner.fork_path = fork_path;
    }

    pub fn add_child(&self, child_ref: ChildExecutionReference) {
        let mut inner = wf_common::lock::write_ok(self.inner.write());
        let key = format!(
            "{}:{}",
            child_type_str(&child_ref.child_type),
            child_ref.child_id
        );
        inner.children.insert(key, child_ref);
    }

    pub fn remove_child(&self, child_id: &str, child_type: &ExecutionType) -> bool {
        let mut inner = wf_common::lock::write_ok(self.inner.write());
        let key = format!("{}:{}", child_type_str(child_type), child_id);
        inner.children.remove(&key).is_some()
    }

    pub fn children(&self) -> Vec<ChildExecutionReference> {
        read_ok(self.inner.read())
            .children
            .values()
            .cloned()
            .collect()
    }

    pub fn depth(&self) -> u32 {
        wf_common::lock::read_ok(self.inner.read()).depth
    }

    pub fn root_execution_id(&self) -> Id {
        wf_common::lock::read_ok(self.inner.read())
            .root_execution_id
            .clone()
    }

    pub fn root_execution_type(&self) -> ExecutionType {
        wf_common::lock::read_ok(self.inner.read())
            .root_execution_type
            .clone()
    }

    /// Set the root-to-parent execution id chain (oldest first, excluding
    /// self). Callers that know the full ancestry (e.g. a parent execution
    /// passing its own chain when spawning a child) use this so
    /// `to_metadata` can carry the chain across processes.
    pub fn set_ancestors(&self, ancestors: Vec<Id>) {
        let mut inner = wf_common::lock::write_ok(self.inner.write());
        inner.ancestors = ancestors;
    }

    /// The root-to-parent execution id chain (oldest first, excluding
    /// self). Empty for roots or when only the direct parent is known.
    pub fn ancestors(&self) -> Vec<Id> {
        wf_common::lock::read_ok(self.inner.read())
            .ancestors
            .clone()
    }

    /// The FORK branch this execution was produced by, when it was spawned
    /// from a fork node.
    pub fn fork_path(&self) -> Option<ForkPath> {
        read_ok(self.inner.read()).fork_path.clone()
    }

    pub fn would_create_cycle(&self, ancestor_chain: &[Id]) -> bool {
        let inner = wf_common::lock::read_ok(self.inner.read());

        for ancestor_id in ancestor_chain {
            if *ancestor_id == inner.execution_id {
                return true;
            }
        }
        false
    }
}

impl HierarchyInner {
    fn recalculate(&mut self) {
        if self.parent.is_none() {
            self.depth = 0;
            self.root_execution_id = self.execution_id.clone();
            self.root_execution_type = self.execution_type.clone();
            self.ancestors.clear();
        } else if self.depth == 0 {
            let repaired = if self.ancestors.is_empty() {
                1
            } else {
                self.ancestors.len() as u32
            };
            self.depth = repaired;
            if let Some(root) = self.ancestors.first().cloned() {
                self.root_execution_id = root;
            } else if let Some(parent) = self.parent.as_ref() {
                self.root_execution_id = parent.parent_id.clone();
                self.root_execution_type = parent.parent_type.clone();
            }
        }
    }
}

fn child_type_str(t: &ExecutionType) -> &str {
    match t {
        ExecutionType::Workflow => "workflow",
        ExecutionType::AgentLoop => "agent_loop",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ref(id: &str, child_type: ExecutionType) -> ChildExecutionReference {
        ChildExecutionReference {
            child_type,
            child_id: id.to_string(),
            created_at: wf_common::time::now(),
            fork_path: None,
        }
    }

    /// The persisted shape a manager is rebuilt from: forward links only.
    fn record_of(manager: &ExecutionHierarchyManager) -> wf_types::execution::ExecutionHierarchy {
        let parent = manager.parent();
        let ancestors = manager.ancestors();
        wf_types::execution::ExecutionHierarchy {
            workflow_id: manager.execution_id(),
            execution_id: manager.execution_id(),
            parent_execution_id: parent.as_ref().map(|p| p.parent_id.clone()),
            parent_execution_type: parent.as_ref().map(|p| p.parent_type.clone()),
            depth: manager.depth(),
            root_execution_id: Some(manager.root_execution_id()),
            root_execution_type: Some(manager.root_execution_type()),
            ancestors: if ancestors.is_empty() {
                None
            } else {
                Some(ancestors)
            },
            fork_path: manager.fork_path(),
        }
    }

    #[test]
    fn test_new_is_root() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        assert_eq!(m.depth(), 0);
        assert_eq!(m.root_execution_id(), "exec1");
        assert!(m.parent().is_none());
    }

    #[test]
    fn test_add_and_remove_child() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        m.add_child(make_ref("child1", ExecutionType::AgentLoop));

        assert_eq!(m.children().len(), 1);

        assert!(m.remove_child("child1", &ExecutionType::AgentLoop));
        assert!(m.children().is_empty());
        assert!(!m.remove_child("child1", &ExecutionType::AgentLoop));
    }

    #[test]
    fn test_set_parent() {
        let m = ExecutionHierarchyManager::new("child_exec".to_string(), ExecutionType::Workflow);
        m.set_parent(
            ParentExecutionContext {
                parent_id: "parent_exec".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            None,
        )
        .unwrap();

        let parent = m.parent().unwrap();
        assert_eq!(parent.parent_id, "parent_exec");
    }

    #[test]
    fn test_set_self_as_parent_fails() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        let result = m.set_parent(
            ParentExecutionContext {
                parent_id: "exec1".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_set_parent_propagates_ancestors() {
        let m = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::AgentLoop);
        m.set_parent(
            ParentExecutionContext {
                parent_id: "parent".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            Some(&["root".to_string()]),
        )
        .unwrap();

        assert_eq!(
            m.ancestors(),
            vec!["root".to_string(), "parent".to_string()]
        );
        assert_eq!(
            record_of(&m).ancestors,
            Some(vec!["root".to_string(), "parent".to_string()])
        );
    }

    #[test]
    fn test_set_parent_without_ancestors_keeps_chain_untouched() {
        let m = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::Workflow);
        m.set_parent(
            ParentExecutionContext {
                parent_id: "parent".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            None,
        )
        .unwrap();
        assert!(m.ancestors().is_empty());
        assert!(record_of(&m).ancestors.is_none());
    }

    #[test]
    fn test_set_parent_dedups_chain_tail() {
        let m = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::AgentLoop);
        m.set_parent(
            ParentExecutionContext {
                parent_id: "parent".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            Some(&["root".to_string(), "parent".to_string()]),
        )
        .unwrap();
        assert_eq!(
            m.ancestors(),
            vec!["root".to_string(), "parent".to_string()]
        );
    }

    #[test]
    fn test_three_level_chain_roundtrip_through_record() {
        // root -> child -> grandchild: the grandchild's chain is the child's
        // chain extended by the child id, and survives a record roundtrip.
        // The child knows its parent but the parent (root) has no chain,
        // expressed as an explicit empty slice rather than `None`.
        let child = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::AgentLoop);
        child
            .set_parent(
                ParentExecutionContext {
                    parent_id: "root".to_string(),
                    parent_type: ExecutionType::Workflow,
                },
                Some(&[]),
            )
            .unwrap();
        assert_eq!(child.ancestors(), vec!["root".to_string()]);

        let grandchild =
            ExecutionHierarchyManager::new("grandchild".to_string(), ExecutionType::AgentLoop);
        grandchild
            .set_parent(
                ParentExecutionContext {
                    parent_id: "child".to_string(),
                    parent_type: ExecutionType::AgentLoop,
                },
                Some(&child.ancestors()),
            )
            .unwrap();

        assert_eq!(
            grandchild.ancestors(),
            vec!["root".to_string(), "child".to_string()]
        );

        let restored = ExecutionHierarchyManager::restore(
            "grandchild".to_string(),
            ExecutionType::AgentLoop,
            &record_of(&grandchild),
            ExecutionType::AgentLoop,
        );
        assert_eq!(
            restored.ancestors(),
            vec!["root".to_string(), "child".to_string()]
        );
        assert_eq!(restored.depth(), 2);
        assert_eq!(restored.root_execution_id(), "root");
    }

    #[test]
    fn test_would_create_cycle() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        assert!(m.would_create_cycle(&["exec1".to_string()]));
        assert!(!m.would_create_cycle(&["other".to_string()]));
    }

    #[test]
    fn test_restore_carries_no_children() {
        // Children are discovered from the child records, so a manager
        // rebuilt from a persisted record starts with an empty child list
        // even when the live manager had registered children.
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::AgentLoop);
        m.add_child(make_ref("c1", ExecutionType::Workflow));
        m.add_child(make_ref("c2", ExecutionType::AgentLoop));
        assert_eq!(m.children().len(), 2);

        let restored = ExecutionHierarchyManager::restore(
            "exec1".to_string(),
            ExecutionType::AgentLoop,
            &record_of(&m),
            ExecutionType::AgentLoop,
        );
        assert!(restored.children().is_empty());
        assert_eq!(restored.root_execution_type(), ExecutionType::AgentLoop);
    }

    #[test]
    fn test_ancestors_roundtrip_through_record() {
        let m = ExecutionHierarchyManager::new("root".to_string(), ExecutionType::Workflow);
        m.set_ancestors(vec!["parent-1".to_string(), "parent-2".to_string()]);

        let record = record_of(&m);
        assert_eq!(
            record.ancestors,
            Some(vec!["parent-1".to_string(), "parent-2".to_string()])
        );

        let restored = ExecutionHierarchyManager::restore(
            "child".to_string(),
            ExecutionType::Workflow,
            &record,
            ExecutionType::Workflow,
        );
        assert_eq!(
            restored.ancestors(),
            vec!["parent-1".to_string(), "parent-2".to_string()]
        );
    }

    #[test]
    fn test_root_has_no_ancestors() {
        let m = ExecutionHierarchyManager::new("root".to_string(), ExecutionType::AgentLoop);
        assert!(m.ancestors().is_empty());
        assert!(record_of(&m).ancestors.is_none());
    }

    #[test]
    fn test_child_key_collision_different_types() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        m.add_child(make_ref("same_id", ExecutionType::Workflow));
        m.add_child(make_ref("same_id", ExecutionType::AgentLoop));

        assert_eq!(m.children().len(), 2);
    }

    #[test]
    fn test_set_parent_assigns_depth_and_root() {
        let m = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::Workflow);
        m.set_parent(
            ParentExecutionContext {
                parent_id: "root".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            Some(&[]),
        )
        .unwrap();
        assert_eq!(m.depth(), 1);
        assert_eq!(m.root_execution_id(), "root");
        assert_eq!(m.ancestors(), vec!["root".to_string()]);
    }

    #[test]
    fn test_set_parent_grows_depth_along_chain() {
        let child = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::Workflow);
        child
            .set_parent(
                ParentExecutionContext {
                    parent_id: "root".to_string(),
                    parent_type: ExecutionType::Workflow,
                },
                Some(&[]),
            )
            .unwrap();
        assert_eq!(child.depth(), 1);

        let grandchild =
            ExecutionHierarchyManager::new("grandchild".to_string(), ExecutionType::Workflow);
        grandchild
            .set_parent(
                ParentExecutionContext {
                    parent_id: "child".to_string(),
                    parent_type: ExecutionType::Workflow,
                },
                Some(&child.ancestors()),
            )
            .unwrap();
        assert_eq!(grandchild.depth(), 2);
        assert_eq!(grandchild.root_execution_id(), "root");
        assert_eq!(
            grandchild.ancestors(),
            vec!["root".to_string(), "child".to_string()]
        );
    }

    #[test]
    fn test_set_parent_full_uses_parent_depth_and_root() {
        let m = ExecutionHierarchyManager::new("child".to_string(), ExecutionType::AgentLoop);
        m.set_parent_full(
            ParentExecutionContext {
                parent_id: "parent".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            &["root".to_string()],
            1,
            "root".to_string(),
            ExecutionType::Workflow,
        )
        .unwrap();
        assert_eq!(m.depth(), 2);
        assert_eq!(m.root_execution_id(), "root");
        assert_eq!(m.root_execution_type(), ExecutionType::Workflow);
        assert_eq!(
            m.ancestors(),
            vec!["root".to_string(), "parent".to_string()]
        );
    }

    #[test]
    fn test_set_parent_rejects_beyond_max_depth() {
        let m = ExecutionHierarchyManager::new("deep".to_string(), ExecutionType::Workflow);
        let long: Vec<String> = (0..MAX_DEPTH).map(|i| format!("a{i}")).collect();
        let refs: Vec<Id> = long;
        let result = m.set_parent(
            ParentExecutionContext {
                parent_id: "parent".to_string(),
                parent_type: ExecutionType::Workflow,
            },
            Some(&refs),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_restore_keeps_own_execution_type() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::AgentLoop);
        m.add_child(make_ref("c1", ExecutionType::Workflow));
        let restored = ExecutionHierarchyManager::restore(
            "exec1".to_string(),
            ExecutionType::AgentLoop,
            &record_of(&m),
            ExecutionType::AgentLoop,
        );
        assert_eq!(restored.root_execution_type(), ExecutionType::AgentLoop);
    }

    #[test]
    fn test_derive_child_carries_depth_root_ancestors() {
        let parent = Arc::new(ExecutionHierarchyManager::new(
            "root".to_string(),
            ExecutionType::Workflow,
        ));
        let child = parent
            .derive_child("child".to_string(), ExecutionType::Workflow, None)
            .unwrap();
        assert_eq!(child.depth(), 1);
        assert_eq!(child.root_execution_id(), "root");
        assert_eq!(child.ancestors(), vec!["root".to_string()]);
        assert_eq!(parent.children().len(), 1);
        assert!(child.fork_path().is_none());
    }

    #[test]
    fn test_derive_child_registers_fork_path() {
        let parent = Arc::new(ExecutionHierarchyManager::new(
            "root".to_string(),
            ExecutionType::Workflow,
        ));
        let fork = ForkPath::new("fork-1", "path-a");
        let child = parent
            .derive_child("branch-1".to_string(), ExecutionType::Workflow, Some(fork))
            .unwrap();
        assert_eq!(child.depth(), 1);
        let children = parent.children();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].fork_node_id(), Some("fork-1"));
        assert_eq!(children[0].branch_path_id(), Some("path-a"));
        // The child also keeps its own provenance, so a record written from
        // the child alone still reports which branch produced it.
        assert_eq!(child.fork_path().unwrap().fork_node_id(), "fork-1");
        assert_eq!(child.fork_path().unwrap().branch_path_id(), "path-a");
    }

    #[test]
    fn test_derive_child_rejects_self_and_beyond_max_depth() {
        let parent = Arc::new(ExecutionHierarchyManager::new(
            "root".to_string(),
            ExecutionType::Workflow,
        ));
        assert!(parent
            .derive_child("root".to_string(), ExecutionType::Workflow, None)
            .is_err());
        let mut current = parent;
        for i in 0..MAX_DEPTH {
            let next = current
                .derive_child(format!("deep-{i}"), ExecutionType::Workflow, None)
                .unwrap();
            current = next;
        }
        assert!(current
            .derive_child("too-deep".to_string(), ExecutionType::Workflow, None)
            .is_err());
    }
}
