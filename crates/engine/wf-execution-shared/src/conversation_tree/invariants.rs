//! Structural invariants of a conversation tree.
//!
//! Checked on load and before every surgery: a corrupt tree must be rejected
//! loudly rather than silently projected into a wrong context.

use std::collections::HashSet;

use wf_types::{ConversationTree, NodeId};

/// Hard ceiling on one tree document. Keeps the single-document write bounded
/// and the tree panel renderable.
pub const MAX_TREE_NODES: usize = 2000;

/// Hard ceiling on path length. A deeper tree would also blow the context
/// window long before it becomes useful.
pub const MAX_TREE_DEPTH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TreeError {
    #[error("node '{0}' is referenced but missing")]
    MissingNode(NodeId),
    #[error("node '{id}' points at missing parent '{parent}'")]
    MissingParent { id: NodeId, parent: NodeId },
    #[error("tree contains a cycle through node '{0}'")]
    Cycle(NodeId),
    #[error("node '{0}' is not reachable from the root")]
    Unreachable(NodeId),
    #[error("sibling ordinal {ordinal} is duplicated under '{parent}'")]
    DuplicateOrdinal { parent: NodeId, ordinal: u64 },
    #[error("active leaf '{0}' does not exist")]
    BadActiveLeaf(NodeId),
    #[error("premise node '{0}' must not carry a reply")]
    PremiseWithReply(NodeId),
    #[error("tree holds {count} nodes, over the limit of {limit}")]
    TooManyNodes { count: usize, limit: usize },
    #[error("tree depth {depth} exceeds the limit of {limit}")]
    TooDeep { depth: usize, limit: usize },
    #[error("no surgery can be undone")]
    NoUndo,
    #[error("undo snapshot is invalid: {0}")]
    BadSnapshot(String),
}

/// Validate the whole document.
pub fn validate(tree: &ConversationTree) -> Result<(), TreeError> {
    if tree.nodes.len() > MAX_TREE_NODES {
        return Err(TreeError::TooManyNodes {
            count: tree.nodes.len(),
            limit: MAX_TREE_NODES,
        });
    }
    if let Some(leaf) = tree.active_leaf.as_deref() {
        if leaf != tree.root && !tree.nodes.contains_key(leaf) {
            return Err(TreeError::BadActiveLeaf(leaf.to_string()));
        }
    }

    let mut seen: HashSet<&str> = HashSet::with_capacity(tree.nodes.len());
    for node in tree.nodes.values() {
        if !seen.insert(node.id.as_str()) {
            return Err(TreeError::Cycle(node.id.clone()));
        }
        if node.parent != tree.root && !tree.nodes.contains_key(&node.parent) {
            return Err(TreeError::MissingParent {
                id: node.id.clone(),
                parent: node.parent.clone(),
            });
        }
        if node.kind == wf_types::NodeKind::Premise && node.reply.is_some() {
            return Err(TreeError::PremiseWithReply(node.id.clone()));
        }
        if let Some(chain) = tree.path_to(&node.id) {
            if chain.len() > MAX_TREE_DEPTH {
                return Err(TreeError::TooDeep {
                    depth: chain.len(),
                    limit: MAX_TREE_DEPTH,
                });
            }
        } else {
            return Err(TreeError::Cycle(node.id.clone()));
        }
    }

    // Sibling ordinals must be unique per parent.
    let mut ordinals: HashSet<(&str, u64)> = HashSet::new();
    for node in tree.nodes.values() {
        if !ordinals.insert((node.parent.as_str(), node.ordinal)) {
            return Err(TreeError::DuplicateOrdinal {
                parent: node.parent.clone(),
                ordinal: node.ordinal,
            });
        }
    }
    Ok(())
}

/// Whether moving `node` under `new_parent` would create a cycle or a
/// self-parenting node.
pub fn would_cycle(tree: &ConversationTree, node: &str, new_parent: &str) -> bool {
    if new_parent == node {
        return true;
    }
    tree.subtree_contains(node, new_parent)
}

/// Depth of the deepest path under `root` (that node itself counts as 1).
pub fn subtree_depth(tree: &ConversationTree, root: &str) -> usize {
    let mut stack = vec![(root.to_string(), 1usize)];
    let mut deepest = 0usize;
    while let Some((current, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        for child in tree.child_ids(&current) {
            stack.push((child, depth + 1));
        }
    }
    deepest
}
