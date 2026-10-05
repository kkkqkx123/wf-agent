use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionType {
    Workflow,
    AgentLoop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionIdentity {
    pub r#type: ExecutionType,
    pub id: super::super::Id,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionHierarchy {
    pub workflow_id: super::super::Id,
    pub execution_id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_type: Option<ExecutionType>,
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_execution_id: Option<super::super::Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_execution_type: Option<ExecutionType>,
    /// Root-to-parent execution id chain (oldest first, excluding self).
    /// Carried through checkpoints so deep hierarchies survive
    /// cross-process restore; `None` when the chain is unknown (legacy
    /// data or a root execution).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestors: Option<Vec<super::super::Id>>,
    /// How this execution came to exist, when it was produced by a FORK
    /// branch. Creation provenance is a forward fact about this execution,
    /// so it lives here rather than as a reverse entry on the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_path: Option<ForkPath>,
}

/// Delimiter wrapping every id in a materialised execution path.
pub const PATH_DELIMITER: char = '/';

impl ExecutionHierarchy {
    /// The id chain of this execution from the root of its tree down to and
    /// including itself.
    ///
    /// A root is its own only ancestor. When the parent is known but the longer
    /// chain is not, the chain is just that parent, which keeps chain, depth
    /// and root mutually consistent rather than describing a different tree.
    pub fn chain(&self) -> Vec<String> {
        let ancestors = match self.parent_execution_id.as_ref() {
            None => Vec::new(),
            Some(parent) => match self.ancestors.as_ref() {
                Some(chain) if !chain.is_empty() => {
                    chain.iter().map(|id| id.to_string()).collect()
                }
                _ => vec![parent.to_string()],
            },
        };
        ancestors
            .into_iter()
            .chain(std::iter::once(self.execution_id.to_string()))
            .collect()
    }

    /// The materialised form of [`Self::chain`]: every id wrapped in the path
    /// delimiter. One prefix scan on it answers "everything under this root",
    /// and cutting its last segment answers "who are my ancestors".
    ///
    /// The delimiter after the last id is what keeps a scan for `/r/` from
    /// matching a different tree whose root id merely begins the same way.
    pub fn path(&self) -> String {
        encode_path(&self.chain())
    }
}

/// Build the materialised path of a chain given root first, self last.
pub fn encode_path(chain: &[String]) -> String {
    let mut path = String::new();
    for id in chain {
        path.push(PATH_DELIMITER);
        path.push_str(id);
    }
    path.push(PATH_DELIMITER);
    path
}

/// Split a materialised path back into its ids, root first, self last.
pub fn decode_path(path: &str) -> Vec<String> {
    path.split(PATH_DELIMITER)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Id;

    fn hierarchy(parent: Option<&str>, ancestors: Option<&[&str]>) -> ExecutionHierarchy {
        let ids: Option<Vec<Id>> =
            ancestors.map(|chain| chain.iter().map(|id| Id::from(*id)).collect());
        let root = ancestors
            .and_then(|chain| chain.first())
            .copied()
            .or(parent)
            .unwrap_or("self");
        ExecutionHierarchy {
            workflow_id: "wf-1".into(),
            execution_id: "self".into(),
            parent_execution_id: parent.map(Id::from),
            parent_execution_type: parent.map(|_| ExecutionType::AgentLoop),
            depth: ancestors.map_or(u32::from(parent.is_some()), |a| a.len() as u32),
            root_execution_id: Some(Id::from(root)),
            root_execution_type: Some(ExecutionType::Workflow),
            ancestors: ids,
            fork_path: None,
        }
    }

    #[test]
    fn a_root_is_its_own_only_ancestor() {
        let root = hierarchy(None, None);
        assert_eq!(root.chain(), vec!["self".to_string()]);
        assert_eq!(root.path(), "/self/");
        assert_eq!(decode_path(&root.path()), vec!["self".to_string()]);
    }

    #[test]
    fn a_child_path_extends_its_parents() {
        let child = hierarchy(Some("root"), Some(&["root"]));
        assert_eq!(child.path(), "/root/self/");
        assert_eq!(decode_path(&child.path()), child.chain());
    }

    /// A record that knows its parent but not the longer chain still describes
    /// one consistent tree instead of a truncated one.
    #[test]
    fn a_known_parent_alone_yields_a_consistent_chain() {
        let orphan = hierarchy(Some("known-parent"), None);
        assert_eq!(orphan.chain(), vec!["known-parent", "self"]);
        assert_eq!(decode_path(&orphan.path()).len() as u32, orphan.depth + 1);
    }

    #[test]
    fn path_roundtrips_through_every_level() {
        for level in 0..4usize {
            let ancestors: Vec<String> = (0..level).map(|i| format!("a{i}")).collect();
            let mut record = hierarchy(ancestors.last().map(String::as_str), None);
            record.execution_id = "leaf".into();
            record.ancestors = Some(ancestors.iter().map(Id::from).collect());
            record.depth = level as u32;
            assert_eq!(decode_path(&record.path()), record.chain());
            assert_eq!(decode_path(&record.path()).len() as u32 - 1, record.depth);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkPath {
    pub fork_node_id: String,
    pub branch_path_id: String,
}

impl ForkPath {
    pub fn new(fork_node_id: impl Into<String>, branch_path_id: impl Into<String>) -> Self {
        Self {
            fork_node_id: fork_node_id.into(),
            branch_path_id: branch_path_id.into(),
        }
    }

    pub fn branch_path_id(&self) -> &str {
        &self.branch_path_id
    }

    pub fn fork_node_id(&self) -> &str {
        &self.fork_node_id
    }
}
