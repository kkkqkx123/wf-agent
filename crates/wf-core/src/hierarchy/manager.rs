use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use wf_types::execution::{ChildExecutionReference, ExecutionType};
use wf_types::Id;

use crate::error::{CoreError, CoreResult};

pub const MAX_DEPTH: u32 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentExecutionContext {
    pub parent_id: Id,
    pub parent_type: ExecutionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionHierarchyMetadata {
    pub parent: Option<ParentExecutionContext>,
    pub children: Vec<ChildExecutionReference>,
    pub depth: u32,
    pub root_execution_id: Id,
    pub root_execution_type: ExecutionType,
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
            }),
        }
    }

    pub fn from_metadata(
        execution_id: Id,
        _execution_type: ExecutionType,
        metadata: ExecutionHierarchyMetadata,
    ) -> Self {
        let children: HashMap<String, ChildExecutionReference> = metadata
            .children
            .into_iter()
            .map(|c| (format!("{}:{}", child_type_str(&c.child_type), c.child_id), c))
            .collect();

        Self {
            inner: RwLock::new(HierarchyInner {
                execution_id,
                execution_type: metadata.root_execution_type.clone(),
                parent: metadata.parent,
                children,
                depth: metadata.depth,
                root_execution_id: metadata.root_execution_id,
                root_execution_type: metadata.root_execution_type,
            }),
        }
    }

    pub fn set_parent(&self, parent: ParentExecutionContext) -> CoreResult<()> {
        if parent.parent_id == self.inner.read().unwrap().execution_id {
            return Err(CoreError::StateError(format!(
                "cannot set self ({}) as parent",
                parent.parent_id
            )));
        }

        let mut inner = self.inner.write().unwrap();
        let new_depth = inner.estimate_parent_depth(&parent) + 1;

        if new_depth > MAX_DEPTH {
            return Err(CoreError::StateError(format!(
                "maximum hierarchy depth exceeded: {} > {}",
                new_depth, MAX_DEPTH
            )));
        }

        inner.parent = Some(parent);
        inner.recalculate();

        Ok(())
    }

    pub fn parent(&self) -> Option<ParentExecutionContext> {
        self.inner.read().unwrap().parent.clone()
    }

    pub fn add_child(&self, child_ref: ChildExecutionReference) {
        let mut inner = self.inner.write().unwrap();
        let key = format!("{}:{}", child_type_str(&child_ref.child_type), child_ref.child_id);
        inner.children.insert(key, child_ref);
    }

    pub fn remove_child(&self, child_id: &str, child_type: &ExecutionType) -> bool {
        let mut inner = self.inner.write().unwrap();
        let key = format!("{}:{}", child_type_str(child_type), child_id);
        inner.children.remove(&key).is_some()
    }

    pub fn children(&self) -> Vec<ChildExecutionReference> {
        self.inner.read().unwrap().children.values().cloned().collect()
    }

    pub fn depth(&self) -> u32 {
        self.inner.read().unwrap().depth
    }

    pub fn root_execution_id(&self) -> Id {
        self.inner.read().unwrap().root_execution_id.clone()
    }

    pub fn root_execution_type(&self) -> ExecutionType {
        self.inner
            .read()
            .unwrap()
            .root_execution_type
            .clone()
    }

    pub fn to_metadata(&self) -> ExecutionHierarchyMetadata {
        let inner = self.inner.read().unwrap();
        ExecutionHierarchyMetadata {
            parent: inner.parent.clone(),
            children: inner.children.values().cloned().collect(),
            depth: inner.depth,
            root_execution_id: inner.root_execution_id.clone(),
            root_execution_type: inner.root_execution_type.clone(),
        }
    }

    pub fn would_create_cycle(&self, ancestor_chain: &[Id]) -> bool {
        let inner = self.inner.read().unwrap();

        for ancestor_id in ancestor_chain {
            if *ancestor_id == inner.execution_id {
                return true;
            }
        }
        false
    }
}

impl HierarchyInner {
    fn estimate_parent_depth(&self, _parent: &ParentExecutionContext) -> u32 {
        self.depth.saturating_sub(1)
    }

    fn recalculate(&mut self) {
        if self.parent.is_none() {
            self.depth = 0;
            self.root_execution_id = self.execution_id.clone();
            self.root_execution_type = self.execution_type.clone();
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
            fork_path_id: None,
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
        m.set_parent(ParentExecutionContext {
            parent_id: "parent_exec".to_string(),
            parent_type: ExecutionType::Workflow,
        })
        .unwrap();

        let parent = m.parent().unwrap();
        assert_eq!(parent.parent_id, "parent_exec");
    }

    #[test]
    fn test_set_self_as_parent_fails() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        let result = m.set_parent(ParentExecutionContext {
            parent_id: "exec1".to_string(),
            parent_type: ExecutionType::Workflow,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_would_create_cycle() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        assert!(m.would_create_cycle(&["exec1".to_string()]));
        assert!(!m.would_create_cycle(&["other".to_string()]));
    }

    #[test]
    fn test_to_metadata_roundtrip() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::AgentLoop);
        m.add_child(make_ref("c1", ExecutionType::Workflow));
        m.add_child(make_ref("c2", ExecutionType::AgentLoop));

        let metadata = m.to_metadata();
        assert_eq!(metadata.children.len(), 2);
        assert_eq!(metadata.root_execution_id, "exec1");

        let restored =
            ExecutionHierarchyManager::from_metadata("exec1".to_string(), ExecutionType::AgentLoop, metadata);
        assert_eq!(restored.children().len(), 2);
        assert_eq!(restored.root_execution_type(), ExecutionType::AgentLoop);
    }

    #[test]
    fn test_child_key_collision_different_types() {
        let m = ExecutionHierarchyManager::new("exec1".to_string(), ExecutionType::Workflow);
        m.add_child(make_ref("same_id", ExecutionType::Workflow));
        m.add_child(make_ref("same_id", ExecutionType::AgentLoop));

        assert_eq!(m.children().len(), 2);
    }
}
