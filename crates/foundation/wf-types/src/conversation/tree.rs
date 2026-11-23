use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

/// Id of the virtual root. It carries no message and exists so a tree always
/// has exactly one node whose parent is absent.
pub const CONVERSATION_ROOT: &str = "root";

/// Document version, so a later shape change can migrate instead of guessing.
pub const CONVERSATION_TREE_SCHEMA_VERSION: u16 = 1;

/// Number of `OpRecord`s kept for undo. Old records fall off the front.
pub const CONVERSATION_MAX_OP_RECORDS: usize = 200;

pub type NodeId = String;

/// Which document this is: a session's main tree, or a detached subtree that
/// waits in the session's graft pool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeKind {
    #[default]
    Session,
    /// A subtree cut out of a session tree. It keeps its own root and can be
    /// grafted back into the same session or into another one.
    Graft,
}

/// What one node contributes to the context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// One question/answer turn.
    #[default]
    Turn,
    /// A pinned premise: input only, never answered, never generated.
    Premise,
}

/// Lifecycle of the reply a turn carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyState {
    /// In flight: the driver is generating it.
    Pending,
    /// Default terminal state. Only `Kept` replies enter the context, so only
    /// they can go dirty.
    Kept,
    /// The user decided not to keep this answer. History stays visible.
    Discarded,
    /// Cancelled or cut off; partial text may exist.
    Interrupted,
    /// Generation failed; no usable content.
    Failed,
}

impl ReplyState {
    /// Whether the reply (and its tool trace) is part of the context and of
    /// the digest a child node folds on.
    pub fn in_context(self) -> bool {
        matches!(self, Self::Kept)
    }

    /// Whether a stale `based_on` on this reply means "should be recomputed".
    pub fn recomputable(self) -> bool {
        matches!(self, Self::Kept)
    }
}

impl Default for ReplyState {
    fn default() -> Self {
        Self::Pending
    }
}

/// Who authored the body currently in place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyOrigin {
    #[default]
    Model,
    /// A human rewrote the body. Regeneration must not silently overwrite it.
    Human,
}

/// A stored message plus the hash of its content.
///
/// The hash is what makes the tree self-sufficient: freshness is computed from
/// tree documents alone, because message bodies live in another table and are
/// immutable once written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRef {
    pub id: Id,
    /// `blake3` over the message JSON minus volatile keys, 32 hex chars.
    pub hash: String,
}

impl MessageRef {
    pub fn new(id: impl Into<Id>, hash: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            hash: hash.into(),
        }
    }
}

/// Model-side parameters recorded with a reply. Advisory only: a profile or
/// parameter change surfaces as `model_drift`, never as `dirty`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamsFingerprint {
    pub profile_id: String,
    /// Serialized generation parameters that affect output sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<String>,
    /// Tool allow-list in effect for the turn, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

/// The answer attached to a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// Body currently in place.
    pub message: MessageRef,
    /// Tool round trip (assistant `tool_calls` then `tool` results), in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<MessageRef>,
    pub state: ReplyState,
    #[serde(default)]
    pub origin: ReplyOrigin,
    /// Prefix digest the body was generated against.
    pub based_on: String,
    /// Tree version at generation time, for "stale since vN" display.
    pub based_on_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<ParamsFingerprint>,
    /// Digest the user explicitly accepted while knowing it was stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged: Option<String>,
    /// Bodies this one replaced (regeneration or rewrite), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded: Vec<MessageRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<Timestamp>,
    pub generated_at: Timestamp,
}

impl Reply {
    /// A reply that is being generated: the body is a placeholder until the
    /// driver writes the real message.
    pub fn pending(message: MessageRef, params: Option<ParamsFingerprint>, now: Timestamp) -> Self {
        Self {
            message,
            trace: Vec::new(),
            state: ReplyState::Pending,
            origin: ReplyOrigin::Model,
            based_on: String::new(),
            based_on_version: 0,
            params,
            acknowledged: None,
            superseded: Vec::new(),
            edited_at: None,
            generated_at: now,
        }
    }
}

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: NodeId,
    /// Sibling order. Deliberately excluded from the digest: a surgery that
    /// restores a node must restore freshness too, and renumbering siblings
    /// must not look like a history change.
    pub ordinal: u64,
    pub kind: NodeKind,
    /// Context messages injected ahead of the user message (mention receipts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<MessageRef>,
    pub user: MessageRef,
    /// User messages replaced by an in-place edit of this node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded_user: Vec<MessageRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<Reply>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<Timestamp>,
    /// Tree version at which this node's `base` digest last changed. Not the
    /// digest itself: digests are rebuilt on load, this is the audited stamp.
    #[serde(default)]
    pub base_version: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Node {
    pub fn turn(
        id: NodeId,
        parent: NodeId,
        ordinal: u64,
        user: MessageRef,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            parent,
            ordinal,
            kind: NodeKind::Turn,
            context: Vec::new(),
            user,
            superseded_user: Vec::new(),
            reply: None,
            label: None,
            deleted_at: None,
            base_version: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn premise(
        id: NodeId,
        parent: NodeId,
        ordinal: u64,
        user: MessageRef,
        now: Timestamp,
    ) -> Self {
        Self {
            kind: NodeKind::Premise,
            ..Self::turn(id, parent, ordinal, user, now)
        }
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// The reply when it contributes to the context, i.e. when a child's
    /// prefix digest folds it.
    pub fn context_reply(&self) -> Option<&Reply> {
        self.reply.as_ref().filter(|reply| reply.state.in_context())
    }
}

/// Reply half of one folded segment: the body hash plus the tool trace hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentReply {
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<String>,
}

/// One segment of the folded prefix, materialized when a subtree is detached.
///
/// Segmenting (instead of compressing the prefix into one hash) is what makes
/// a grafted subtree stay fresh: folding the copied segments from the new root
/// reproduces the original `in_digest` byte for byte.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefixSegment {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<String>,
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<SegmentReply>,
}

/// Where a detached subtree came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraftOrigin {
    pub source_session_id: Id,
    pub source_tree_id: Id,
    pub source_node_id: NodeId,
    pub detached_at: Timestamp,
    /// Ancestor segments copied so the subtree keeps its freshness. Empty when
    /// the caller chose not to carry the prefix, which marks everything dirty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_segments: Vec<PrefixSegment>,
}

/// One entry of the session's graft pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolEntry {
    pub graft_id: Id,
    pub label: Option<String>,
    pub node_count: u64,
    pub detached_at: Timestamp,
    /// Node the subtree used to hang under.
    pub origin_node: NodeId,
}

/// One recorded surgery, kept so an undo can be derived instead of replayed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpRecord {
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeId>,
    pub version_before: u64,
    pub version_after: u64,
    pub at: Timestamp,
}

/// One conversation tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationTree {
    /// Primary key. A session owns exactly one `Session` tree plus zero or more
    /// `Graft` trees sitting in its pool.
    pub id: Id,
    /// Conversation anchor: the chat `session_id`.
    pub session_id: Id,
    pub kind: TreeKind,
    pub root: NodeId,
    pub nodes: BTreeMap<NodeId, Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_leaf: Option<NodeId>,
    pub next_ordinal: u64,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<GraftOrigin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool: Vec<PoolEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<OpRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub schema_version: u16,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ConversationTree {
    /// A fresh session tree: one virtual root, no nodes, no leaf.
    ///
    /// The id is supplied by the caller so this crate needs no id generator;
    /// `wf-execution-shared` mints it with the workspace helper.
    pub fn new_session(
        id: impl Into<Id>,
        session_id: impl Into<Id>,
        profile_id: Option<String>,
        now: Timestamp,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            kind: TreeKind::Session,
            root: CONVERSATION_ROOT.to_string(),
            nodes: BTreeMap::new(),
            active_leaf: None,
            next_ordinal: 1,
            version: 1,
            origin: None,
            pool: Vec::new(),
            ops: Vec::new(),
            profile_id,
            schema_version: CONVERSATION_TREE_SCHEMA_VERSION,
            created_at: now,
            updated_at: now,
        }
    }

    /// An empty Graft document, filled by the detach surgery.
    pub fn new_graft(
        id: impl Into<Id>,
        session_id: impl Into<Id>,
        origin: GraftOrigin,
        profile_id: Option<String>,
        now: Timestamp,
    ) -> Self {
        Self {
            kind: TreeKind::Graft,
            origin: Some(origin),
            ..Self::new_session(id, session_id, profile_id, now)
        }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Children of `parent` in sibling order.
    pub fn children(&self, parent: &str) -> Vec<&Node> {
        let mut children: Vec<&Node> = self
            .nodes
            .values()
            .filter(|node| node.parent == parent)
            .collect();
        children.sort_by_key(|node| node.ordinal);
        children
    }

    pub fn child_ids(&self, parent: &str) -> Vec<NodeId> {
        self.children(parent)
            .into_iter()
            .map(|node| node.id.clone())
            .collect()
    }

    /// Root-to-node chain, root excluded, target included. `None` when the
    /// chain does not reach the root (corrupt parent or cycle).
    pub fn path_to(&self, id: &str) -> Option<Vec<&Node>> {
        let mut chain = Vec::new();
        let mut cursor = id;
        let mut guard = 0usize;
        loop {
            if cursor == self.root {
                chain.reverse();
                return Some(chain);
            }
            let node = self.nodes.get(cursor)?;
            chain.push(node);
            cursor = node.parent.as_str();
            guard += 1;
            if guard > self.nodes.len() + 1 {
                return None;
            }
        }
    }

    /// Every node id under `id`, `id` included, in depth-first order.
    pub fn subtree_ids(&self, id: &str) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id.to_string()];
        while let Some(current) = stack.pop() {
            if self.nodes.contains_key(&current) {
                out.push(current.clone());
            }
            let mut children = self.child_ids(&current);
            children.reverse();
            stack.extend(children);
        }
        out
    }

    /// Whether `candidate` sits inside the subtree rooted at `root`.
    pub fn subtree_contains(&self, root: &str, candidate: &str) -> bool {
        if root == candidate {
            return true;
        }
        match self.path_to(candidate) {
            Some(chain) => chain.iter().any(|node| node.id == root),
            None => false,
        }
    }

    /// Allocate the next sibling ordinal.
    pub fn take_ordinal(&mut self) -> u64 {
        let ordinal = self.next_ordinal;
        self.next_ordinal += 1;
        ordinal
    }

    pub fn pool_entry(&self, graft_id: &str) -> Option<&PoolEntry> {
        self.pool.iter().find(|entry| entry.graft_id == graft_id)
    }

    /// Live node count (soft-deleted nodes still occupy the document).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}
