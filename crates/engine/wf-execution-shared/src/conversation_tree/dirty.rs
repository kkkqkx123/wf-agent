//! Dirty detection: which answers no longer match the history above them.
//!
//! "Dirty" is derived, never stored. A reply stays fresh as long as its
//! `based_on` digest equals the node's current `base` digest; editing an
//! ancestor, discarding a reply, grafting a subtree under a different prefix
//! or regenerating a body all move `base`, so the answer below goes stale
//! without any propagation code.

use wf_types::{ConversationTree, NodeId};

use super::digest::DigestIndex;

/// One stale answer on a path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DirtyEntry {
    pub node_id: NodeId,
    /// 1-based position on the path, counting turns only (premises skipped),
    /// so a UI can say "the 2nd and 3rd answers are stale".
    pub index: u32,
    pub based_on_version: u64,
    /// Tree version at which this node's `base` last moved.
    pub stale_since_version: u64,
    /// Shallowest node on the path carrying `stale_since_version`: the change
    /// that started the staleness.
    pub culprit: Option<NodeId>,
}

/// Staleness of one path, plus the count of stale answers elsewhere.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DirtyList {
    pub leaf: Option<NodeId>,
    pub entries: Vec<DirtyEntry>,
    /// Stale answers off the requested path. Counted, never stepped.
    pub other_branches: usize,
}

impl DirtyList {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Evaluate the path ending at `leaf` (or the active leaf when `None`).
pub fn evaluate(tree: &ConversationTree, index: &DigestIndex, leaf: Option<&str>) -> DirtyList {
    let leaf = match leaf
        .map(str::to_string)
        .or_else(|| tree.active_leaf.clone())
    {
        Some(leaf) => leaf,
        None => return DirtyList::default(),
    };
    let Some(path) = tree.path_to(&leaf) else {
        return DirtyList {
            leaf: Some(leaf),
            ..DirtyList::default()
        };
    };

    let mut entries = Vec::new();
    let mut position = 0u32;
    let mut on_path = std::collections::HashSet::with_capacity(path.len());
    for node in &path {
        on_path.insert(node.id.clone());
        if node.kind == wf_types::NodeKind::Turn {
            position += 1;
        }
        if !index.is_dirty(node) {
            continue;
        }
        let stale_since = node.base_version;
        let culprit = path
            .iter()
            .find(|candidate| candidate.base_version == stale_since)
            .map(|candidate| candidate.id.clone());
        entries.push(DirtyEntry {
            node_id: node.id.clone(),
            index: position,
            based_on_version: node
                .reply
                .as_ref()
                .map(|reply| reply.based_on_version)
                .unwrap_or(0),
            stale_since_version: stale_since,
            culprit,
        });
    }

    let other_branches = tree
        .nodes
        .values()
        .filter(|node| !on_path.contains(&node.id) && index.is_dirty(node))
        .count();

    DirtyList {
        leaf: Some(leaf),
        entries,
        other_branches,
    }
}
