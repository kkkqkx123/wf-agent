//! Read-side views: the tree snapshot, a path's messages and the dirty list.
//!
//! These are the only reads a client needs. A snapshot is the whole tree as a
//! document (shape + state, no bodies), a path is the messages of one branch in
//! order, and the dirty list says which answers on that branch no longer match
//! their history.

use serde::Serialize;
use wf_execution_shared::conversation_tree::{self, DigestIndex};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::{ConversationTree, NodeKind, ReplyState};

pub use wf_execution_shared::conversation_tree::{DirtyEntry, DirtyList};

use super::runtime::{self, InflightSnapshot};
use super::store;
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// One node as the client sees it: identity, shape and state, never the text.
#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub id: String,
    pub parent: String,
    pub ordinal: u64,
    pub kind: &'static str,
    pub user_message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyView>,
    pub deleted: bool,
    /// `true` when this answer no longer matches the history above it.
    pub dirty: bool,
    /// Tree version at which the node's prefix last changed.
    pub base_version: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplyView {
    pub state: &'static str,
    pub message_id: String,
    pub based_on_version: u64,
    pub origin: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<i64>,
    pub superseded: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolView {
    pub graft_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub node_count: u64,
    pub detached_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InflightView {
    pub turn_id: String,
    pub node_id: String,
    pub text: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub tree_id: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_leaf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inflight: Option<InflightView>,
    /// Stale answers on the active path (or the requested one).
    pub dirty: DirtyList,
    pub nodes: Vec<NodeView>,
    pub pool: Vec<PoolView>,
}

fn state_name(state: ReplyState) -> &'static str {
    match state {
        ReplyState::Pending => "pending",
        ReplyState::Kept => "kept",
        ReplyState::Discarded => "discarded",
        ReplyState::Interrupted => "interrupted",
        ReplyState::Failed => "failed",
        ReplyState::Superseded => "superseded",
    }
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Turn => "turn",
        NodeKind::Premise => "premise",
    }
}

fn origin_name(origin: wf_types::ReplyOrigin) -> &'static str {
    match origin {
        wf_types::ReplyOrigin::Model => "model",
        wf_types::ReplyOrigin::Human => "human",
    }
}

/// Node views of a tree, in document order.
pub fn node_views(tree: &ConversationTree, index: &DigestIndex) -> Vec<NodeView> {
    tree.nodes
        .values()
        .map(|node| NodeView {
            id: node.id.clone(),
            parent: node.parent.clone(),
            ordinal: node.ordinal,
            kind: kind_name(node.kind),
            user_message_id: node.user.id.clone(),
            label: node.label.clone(),
            reply: node.reply.as_ref().map(|reply| ReplyView {
                state: state_name(reply.state),
                message_id: reply.message.id.clone(),
                based_on_version: reply.based_on_version,
                origin: origin_name(reply.origin),
                edited_at: reply.edited_at,
                superseded: reply.superseded.iter().map(|m| m.id.clone()).collect(),
            }),
            deleted: node.is_deleted(),
            dirty: index.is_dirty(node),
            base_version: node.base_version,
            created_at: node.created_at,
            updated_at: node.updated_at,
        })
        .collect()
}

/// Full snapshot of one session.
pub async fn snapshot(
    ctx: &ApiContext,
    session_id: &str,
    leaf: Option<&str>,
) -> ApiResult<SessionSnapshot> {
    let tree = store::load_or_adopt(ctx, session_id).await?;
    let index = store::digests(&tree);
    let dirty = conversation_tree::evaluate_dirty(&tree, &index, leaf);
    let inflight: Option<InflightSnapshot> = runtime::runtime_for(session_id).inflight();
    Ok(SessionSnapshot {
        session_id: session_id.to_string(),
        tree_id: tree.id.clone(),
        version: tree.version,
        profile_id: tree.profile_id.clone(),
        active_leaf: tree.active_leaf.clone(),
        inflight: inflight.map(|snapshot| InflightView {
            turn_id: snapshot.turn_id,
            node_id: snapshot.node_id,
            text: snapshot.text,
            reasoning: snapshot.reasoning,
        }),
        dirty,
        nodes: node_views(&tree, &index),
        pool: tree
            .pool
            .iter()
            .map(|entry| PoolView {
                graft_id: entry.graft_id.clone(),
                label: entry.label.clone(),
                node_count: entry.node_count,
                detached_at: entry.detached_at,
            })
            .collect(),
    })
}

/// One message of a projected path.
#[derive(Debug, Clone, Serialize)]
pub struct PathEntry {
    pub node_id: String,
    pub message: wf_types::message::Message,
    /// `context` / `user` / `tool_trace` / `reply`.
    pub role: &'static str,
    /// Whether the message enters the model context.
    pub retained: bool,
    /// Whether the answer of this node is stale.
    pub dirty: bool,
}

fn role_name(role: conversation_tree::ProjectedRole) -> &'static str {
    match role {
        conversation_tree::ProjectedRole::Context => "context",
        conversation_tree::ProjectedRole::User => "user",
        conversation_tree::ProjectedRole::ToolTrace => "tool_trace",
        conversation_tree::ProjectedRole::Reply => "reply",
    }
}

/// Messages of the path ending at `leaf` (default: the active leaf), bodies
/// included.
///
/// A message that the tree points at but the store no longer has is skipped
/// with a warning: the projection is still renderable without it, and dropping
/// the whole path over one missing body would hide the conversation.
pub async fn path_messages(
    ctx: &ApiContext,
    session_id: &str,
    leaf: Option<&str>,
) -> ApiResult<Vec<PathEntry>> {
    let tree = store::load_or_adopt(ctx, session_id).await?;
    let index = store::digests(&tree);
    let Some(leaf) = leaf
        .map(str::to_string)
        .or_else(|| tree.active_leaf.clone())
    else {
        return Ok(Vec::new());
    };
    let projected = conversation_tree::project(&tree, &leaf);
    let mut out = Vec::with_capacity(projected.len());
    for entry in projected {
        let loaded = ctx
            .storage
            .message
            .load(&entry.message.id)
            .await
            .map_err(ApiError::from)?;
        let Some(record) = loaded else {
            tracing::warn!(
                target: "wf_api::conversation",
                session_id,
                message_id = %entry.message.id,
                "message referenced by the tree is missing from the store"
            );
            continue;
        };
        let dirty = tree
            .node(&entry.node_id)
            .is_some_and(|node| index.is_dirty(node));
        out.push(PathEntry {
            node_id: entry.node_id.clone(),
            message: record.message,
            role: role_name(entry.role),
            retained: entry.retained,
            dirty,
        });
    }
    Ok(out)
}

/// Stale answers of one path, with the culprit that made them stale.
pub async fn dirty_list(
    ctx: &ApiContext,
    session_id: &str,
    leaf: Option<&str>,
) -> ApiResult<DirtyList> {
    let tree = store::load_or_adopt(ctx, session_id).await?;
    let index = store::digests(&tree);
    Ok(conversation_tree::evaluate_dirty(&tree, &index, leaf))
}

/// Profile a session generates with: the tree's, else the record's.
pub(crate) async fn session_profile(ctx: &ApiContext, tree: &ConversationTree) -> Option<String> {
    if let Some(profile) = tree.profile_id.clone() {
        return Some(profile);
    }
    ctx.storage
        .agent_execution
        .load(&tree.session_id)
        .await
        .ok()
        .flatten()
        .and_then(|record| record.context)
        .and_then(|context| context.profile_id)
}
