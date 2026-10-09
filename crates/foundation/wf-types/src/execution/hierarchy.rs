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
    /// Which engine owns the direct parent. Absent at a root, as is
    /// [`Self::parent_execution_id`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_execution_type: Option<ExecutionType>,
    /// Which engine owns the root of this execution's tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_execution_type: Option<ExecutionType>,
    /// Materialised ancestor path: every id in the chain from the root down to
    /// and including this execution, each wrapped in [`PATH_DELIMITER`].
    ///
    /// This is the one stored statement of where the execution sits. Depth,
    /// root, parent and the ancestor chain are all read off it, so no pair of
    /// fields on this record can describe a different tree.
    pub path: String,
    /// How this execution came to exist, when it was produced by a FORK
    /// branch. Creation provenance is a forward fact about this execution,
    /// so it lives here rather than as a reverse entry on the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_path: Option<ForkPath>,
}

/// Delimiter wrapping every id in a materialised execution path.
pub const PATH_DELIMITER: char = '/';

/// Whether `id` can be part of a materialised execution path.
///
/// The path is the single statement of where an execution sits, and it is
/// delimited by [`PATH_DELIMITER`], so an id carrying that delimiter would
/// come back from the path as two different ids. An empty id is just as
/// unusable: the path built from it decodes to nothing.
pub fn is_valid_execution_id(id: &str) -> bool {
    !id.is_empty() && !id.contains(PATH_DELIMITER)
}

/// How deep an execution tree may nest, a root sitting at `0`.
///
/// This is the one place the bound is stated: the structural check that rejects
/// over-deep derivation, the actor id encoding that has to represent the same
/// chain, and the sub-agent admission gate all read it.
pub const MAX_EXECUTION_DEPTH: u32 = 10;

impl ExecutionHierarchy {
    /// Build the lineage of one execution.
    ///
    /// `ancestors` is the chain from the root down to the direct parent, oldest
    /// first, and is empty at a root. Everything this record can say about
    /// position derives from it, so a caller cannot produce a record whose
    /// fields disagree. A caller that knows only the direct parent passes a
    /// single-element chain; a caller that knows nothing else passes an empty
    /// one.
    pub fn new(
        workflow_id: super::super::Id,
        execution_id: super::super::Id,
        ancestors: Vec<super::super::Id>,
        parent_execution_type: Option<ExecutionType>,
        root_execution_type: Option<ExecutionType>,
        fork_path: Option<ForkPath>,
    ) -> Self {
        let parent_execution_id = ancestors.last().cloned();
        let path = encode_path(
            &ancestors
                .iter()
                .map(|id| id.to_string())
                .chain(std::iter::once(execution_id.to_string()))
                .collect::<Vec<_>>(),
        );
        Self {
            workflow_id,
            execution_id,
            // A root has no parent, so it cannot have a parent's engine either.
            parent_execution_type: if parent_execution_id.is_some() {
                parent_execution_type
            } else {
                None
            },
            root_execution_type,
            path,
            fork_path,
        }
    }

    /// The id chain from the root down to and including this execution,
    /// oldest first. A root is its own only ancestor.
    pub fn chain(&self) -> Vec<String> {
        decode_path(&self.path)
    }

    /// The materialised form of [`Self::chain`], as stored. One prefix scan on
    /// it answers "everything under this root", and cutting its last segment
    /// answers "who are my ancestors". The delimiter after the last id is what
    /// keeps a scan for `/r/` from matching a different tree whose root id
    /// merely begins the same way.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Root-to-parent id chain, oldest first, excluding this execution.
    pub fn ancestors(&self) -> Vec<super::super::Id> {
        let mut chain = self.chain();
        chain.pop();
        chain
    }

    /// Nesting level below the root; a root execution is `0`.
    pub fn depth(&self) -> u32 {
        self.chain().len().saturating_sub(1) as u32
    }

    /// The execution this one descends from; [`Self::execution_id`] at a root.
    pub fn parent_execution_id(&self) -> Option<super::super::Id> {
        self.ancestors().last().cloned()
    }

    /// The root of this execution's tree, which a root execution is itself.
    pub fn root_execution_id(&self) -> super::super::Id {
        self.chain().into_iter().next().unwrap_or_default()
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

    fn hierarchy(ancestors: &[&str]) -> ExecutionHierarchy {
        let parent_type = if ancestors.is_empty() {
            None
        } else {
            Some(ExecutionType::AgentLoop)
        };
        ExecutionHierarchy::new(
            "wf-1".into(),
            "self".into(),
            ancestors.iter().map(|id| Id::from(*id)).collect(),
            parent_type,
            Some(ExecutionType::Workflow),
            None,
        )
    }

    #[test]
    fn a_root_is_its_own_only_ancestor() {
        let root = hierarchy(&[]);
        assert_eq!(root.chain(), vec!["self".to_string()]);
        assert_eq!(root.path(), "/self/");
        assert_eq!(decode_path(root.path()), vec!["self".to_string()]);
        assert_eq!(root.depth(), 0);
        assert_eq!(root.parent_execution_id(), None);
        assert_eq!(root.parent_execution_type, None);
        assert_eq!(root.root_execution_id(), "self");
    }

    #[test]
    fn a_child_path_extends_its_parents() {
        let child = hierarchy(&["root"]);
        assert_eq!(child.path(), "/root/self/");
        assert_eq!(decode_path(child.path()), child.chain());
        assert_eq!(child.depth(), 1);
        assert_eq!(child.parent_execution_id(), Some(Id::from("root")));
        assert_eq!(child.parent_execution_type, Some(ExecutionType::AgentLoop));
    }

    /// A record that knows its parent but not the longer chain still describes
    /// one consistent tree instead of a truncated one.
    #[test]
    fn a_known_parent_alone_yields_a_consistent_chain() {
        let orphan = hierarchy(&["known-parent"]);
        assert_eq!(
            orphan.chain(),
            vec!["known-parent".to_string(), "self".to_string()]
        );
        assert_eq!(orphan.depth(), 1);
        assert_eq!(orphan.root_execution_id(), "known-parent");
    }

    #[test]
    fn path_roundtrips_through_every_level() {
        for level in 0..4usize {
            let ancestors: Vec<String> = (0..level).map(|i| format!("a{i}")).collect();
            let record = ExecutionHierarchy::new(
                "wf-1".into(),
                "leaf".into(),
                ancestors.iter().map(Id::from).collect(),
                Some(ExecutionType::Workflow),
                Some(ExecutionType::Workflow),
                None,
            );
            assert_eq!(decode_path(record.path()), record.chain());
            assert_eq!(record.depth(), level as u32);
            assert_eq!(
                record.root_execution_id(),
                ancestors.first().cloned().unwrap_or("leaf".into())
            );
        }
    }

    #[test]
    fn a_sibling_tree_reports_its_own_chain_only() {
        let g = ExecutionHierarchy::new(
            "wf-1".into(),
            "g".into(),
            ["r", "a"].iter().map(|id| Id::from(*id)).collect(),
            Some(ExecutionType::Workflow),
            Some(ExecutionType::Workflow),
            None,
        );
        let b = ExecutionHierarchy::new(
            "wf-1".into(),
            "b".into(),
            vec![Id::from("r")],
            Some(ExecutionType::Workflow),
            Some(ExecutionType::Workflow),
            None,
        );
        assert_eq!(g.ancestors(), vec!["r".to_string(), "a".to_string()]);
        assert_eq!(b.ancestors(), vec!["r".to_string()]);
        assert_eq!(g.root_execution_id(), b.root_execution_id());
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
