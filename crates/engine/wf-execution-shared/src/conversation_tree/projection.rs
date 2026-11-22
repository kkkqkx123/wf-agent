//! Path projection: turning a tree path into the message list a model sees.
//!
//! Only the path matters, which is why "attach a branch" is a head switch
//! (`active_leaf`) or a surgery, and why branches that are not on the path can
//! never leak into the context.

use wf_types::{ConversationTree, MessageRef, NodeId};

/// What a projected message is doing in the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectedRole {
    /// Injected ahead of the user message (mention receipts and the like).
    Context,
    User,
    /// Assistant tool call or tool result inside the reply's trace.
    ToolTrace,
    Reply,
}

/// One message of a projected path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedRef {
    pub node_id: NodeId,
    pub message: MessageRef,
    pub role: ProjectedRole,
    /// Whether the message actually enters the context. `false` marks a reply
    /// the caller may still show but must not send.
    pub retained: bool,
}

/// Project the path ending at `leaf`.
///
/// A soft-deleted node contributes nothing itself, but its descendants are
/// projected normally: deleting a middle node must not orphan the branch that
/// hangs below it.
pub fn project(tree: &ConversationTree, leaf: &str) -> Vec<ProjectedRef> {
    let Some(path) = tree.path_to(leaf) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(path.len() * 2);
    for node in path {
        if node.is_deleted() {
            continue;
        }
        for context in &node.context {
            out.push(ProjectedRef {
                node_id: node.id.clone(),
                message: context.clone(),
                role: ProjectedRole::Context,
                retained: true,
            });
        }
        out.push(ProjectedRef {
            node_id: node.id.clone(),
            message: node.user.clone(),
            role: ProjectedRole::User,
            retained: true,
        });
        if node.kind == wf_types::NodeKind::Premise {
            continue;
        }
        let Some(reply) = node.reply.as_ref() else {
            continue;
        };
        if !reply.state.in_context() {
            continue;
        }
        for trace in &reply.trace {
            out.push(ProjectedRef {
                node_id: node.id.clone(),
                message: trace.clone(),
                role: ProjectedRole::ToolTrace,
                retained: true,
            });
        }
        out.push(ProjectedRef {
            node_id: node.id.clone(),
            message: reply.message.clone(),
            role: ProjectedRole::Reply,
            retained: true,
        });
    }
    out
}

/// Project the active leaf's path.
pub fn project_active(tree: &ConversationTree) -> Vec<ProjectedRef> {
    match tree.active_leaf.as_deref() {
        Some(leaf) => project(tree, leaf),
        None => Vec::new(),
    }
}

/// Ordered message ids of a projected path, i.e. the request history.
pub fn project_ids(tree: &ConversationTree, leaf: &str) -> Vec<String> {
    project(tree, leaf)
        .into_iter()
        .map(|entry| entry.message.id)
        .collect()
}
