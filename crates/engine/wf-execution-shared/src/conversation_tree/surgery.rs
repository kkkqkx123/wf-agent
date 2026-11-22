//! Tree surgery: the only places that change the shape of a conversation.
//!
//! Five verbs cover every edit the product needs — `insert_before`,
//! `splice_out`, `reparent`, `detach`, `attach` — plus the content operations
//! (rewrite the question, rewrite the answer, discard, regenerate, finish a
//! generation). "Replace a predecessor" is `attach(mode = replace)`, i.e.
//! detach the old subtree and graft the new one in the same call.
//!
//! Every verb keeps three things consistent in one place: the tree document,
//! the in-memory digest index, and the `base_version` stamp that tells the UI
//! *when* an answer went stale. Doing it here means no caller can bump a
//! version without re-folding, or move a node without marking what it broke.

use wf_types::{
    ConversationNode, ConversationTree, GraftOrigin, MessageRef, NodeId, NodeKind, OpRecord,
    ParamsFingerprint, PoolEntry, Reply, ReplyOrigin, ReplyState, TreeKind,
    CONVERSATION_MAX_OP_RECORDS, CONVERSATION_ROOT,
};

use super::digest::{self, DigestIndex};
use super::invariants::{self, TreeError, MAX_TREE_DEPTH, MAX_TREE_NODES};

/// What one surgery did, from the caller's point of view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SurgeryOutcome {
    /// Tree version after the change.
    pub version: u64,
    /// Nodes whose `base` digest moved, i.e. whose answers may now be stale.
    pub restale: Vec<NodeId>,
    /// Stale answers in the whole tree after the change.
    pub dirty: usize,
}

/// How a graft lands on the target parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachMode {
    /// Add the subtree as a new child.
    Link,
    /// Replace an existing predecessor: detach it first, then link.
    Replace,
}

#[derive(Debug, Clone, Default)]
pub struct AttachOutcome {
    pub outcome: SurgeryOutcome,
    /// The predecessor that was detached to make room, still in the pool.
    pub replaced: Option<ConversationTree>,
    pub moved: usize,
}

#[derive(Debug, Clone)]
pub struct DetachOutcome {
    pub outcome: SurgeryOutcome,
    /// The detached subtree as its own `Graft` document.
    pub graft: ConversationTree,
}

/// Shared, mutable view of one tree plus its digest index.
///
/// Held under the session's write lock, so a request that mutates the tree
/// cannot interleave with another one.
pub struct TreeEditor<'a> {
    tree: &'a mut ConversationTree,
    digests: &'a mut DigestIndex,
    now: wf_types::Timestamp,
}

impl<'a> TreeEditor<'a> {
    pub fn new(
        tree: &'a mut ConversationTree,
        digests: &'a mut DigestIndex,
        now: wf_types::Timestamp,
    ) -> Self {
        Self { tree, digests, now }
    }

    pub fn tree(&self) -> &ConversationTree {
        self.tree
    }

    pub fn digests(&self) -> &DigestIndex {
        self.digests
    }

    pub fn base_of(&self, node: &str) -> Option<&str> {
        self.digests.base(node)
    }

    // ── structure ──────────────────────────────────────────────────────────

    /// Insert `node` between `target` and `target`'s parent. The target's
    /// subtree follows automatically, so "add a premise before the first
    /// question" is one call.
    pub fn insert_before(
        &mut self,
        target: &str,
        node: ConversationNode,
    ) -> Result<SurgeryOutcome, TreeError> {
        let target_node = self
            .tree
            .node(target)
            .ok_or_else(|| TreeError::MissingNode(target.to_string()))?;
        let parent = target_node.parent.clone();
        if self.tree.nodes.len() + 1 > MAX_TREE_NODES {
            return Err(TreeError::TooManyNodes {
                count: self.tree.nodes.len() + 1,
                limit: MAX_TREE_NODES,
            });
        }
        if invariants::subtree_depth(self.tree, target) + 1 > MAX_TREE_DEPTH {
            return Err(TreeError::TooDeep {
                depth: invariants::subtree_depth(self.tree, target) + 1,
                limit: MAX_TREE_DEPTH,
            });
        }

        let ordinal = self.tree.take_ordinal();
        let mut node = node;
        node.parent = parent;
        node.ordinal = ordinal;
        node.updated_at = self.now;
        let inserted = node.id.clone();
        self.tree.nodes.insert(inserted.clone(), node);
        if let Some(target) = self.tree.node_mut(target) {
            target.parent = inserted.clone();
            target.updated_at = self.now;
        }

        let version = self.bump();
        self.record(
            "insert_before",
            Some(target.to_string()),
            vec![inserted.clone()],
        );
        let mut restale = self.refold_node_and_children(&inserted, version);
        restale.retain(|id| id != &inserted);
        Ok(self.outcome(version, restale))
    }

    /// Lift `node`'s children to its parent and leave `node` childless.
    ///
    /// The node stays in the tree: it is no longer on any path, but grafting it
    /// back with `insert_before` restores the original digest, so the answers
    /// below become fresh again with no cleanup.
    pub fn splice_out(&mut self, node: &str) -> Result<SurgeryOutcome, TreeError> {
        if node == self.tree.root {
            return Err(TreeError::MissingNode(node.to_string()));
        }
        let target = self
            .tree
            .node(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let parent = target.parent.clone();
        let children = self.tree.child_ids(node);
        for child in &children {
            let ordinal = self.tree.take_ordinal();
            if let Some(child_node) = self.tree.node_mut(child) {
                child_node.parent = parent.clone();
                child_node.ordinal = ordinal;
                child_node.updated_at = self.now;
            }
        }
        if self.tree.active_leaf.as_deref() == Some(node) {
            self.tree.active_leaf = Some(parent.clone());
        }
        if let Some(target) = self.tree.node_mut(node) {
            target.updated_at = self.now;
        }

        let version = self.bump();
        self.record("splice_out", Some(node.to_string()), children.clone());
        let restale = self.refold_children(&parent, version);
        Ok(self.outcome(version, restale))
    }

    /// Move a subtree under another parent inside the same tree.
    pub fn reparent(&mut self, node: &str, new_parent: &str) -> Result<SurgeryOutcome, TreeError> {
        if self.tree.node(new_parent).is_none() {
            return Err(TreeError::MissingNode(new_parent.to_string()));
        }
        if invariants::would_cycle(self.tree, node, new_parent) {
            return Err(TreeError::Cycle(node.to_string()));
        }
        let depth = invariants::subtree_depth(self.tree, node);
        let parent_depth = self
            .tree
            .path_to(new_parent)
            .map(|chain| chain.len())
            .ok_or_else(|| TreeError::Cycle(new_parent.to_string()))?;
        if depth + parent_depth > MAX_TREE_DEPTH {
            return Err(TreeError::TooDeep {
                depth: depth + parent_depth,
                limit: MAX_TREE_DEPTH,
            });
        }

        let ordinal = self.tree.take_ordinal();
        if let Some(target) = self.tree.node_mut(node) {
            target.parent = new_parent.to_string();
            target.ordinal = ordinal;
            target.updated_at = self.now;
        }

        let version = self.bump();
        self.record("reparent", Some(node.to_string()), vec![node.to_string()]);
        let restale = self.refold_node_and_children(node, version);
        Ok(self.outcome(version, restale))
    }

    /// Cut a subtree out into its own `Graft` document.
    ///
    /// `carry_prefix` copies the ancestor segments into the graft, which keeps
    /// the subtree fresh. Leaving it off marks the whole subtree dirty, because
    /// the history above it really is gone.
    pub fn detach(
        &mut self,
        node: &str,
        carry_prefix: bool,
        label: Option<String>,
        graft_id: impl Into<NodeId>,
    ) -> Result<DetachOutcome, TreeError> {
        if node == self.tree.root {
            return Err(TreeError::MissingNode(node.to_string()));
        }
        let target = self
            .tree
            .node(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let parent = target.parent.clone();
        let mut prefix = Vec::new();
        if carry_prefix {
            if let Some(chain) = self.tree.path_to(node) {
                prefix = chain
                    .iter()
                    .take(chain.len().saturating_sub(1))
                    .map(|ancestor| digest::segment_from_node(ancestor))
                    .collect();
            }
        }

        let ids = self.tree.subtree_ids(node);
        let leaf_inside = self
            .tree
            .active_leaf
            .as_deref()
            .is_some_and(|leaf| ids.iter().any(|id| id == leaf));

        let origin = GraftOrigin {
            source_session_id: self.tree.session_id.clone(),
            source_tree_id: self.tree.id.clone(),
            source_node_id: node.to_string(),
            detached_at: self.now,
            prefix_segments: prefix,
        };
        let mut graft = ConversationTree::new_graft(
            graft_id.into(),
            self.tree.session_id.clone(),
            origin,
            self.tree.profile_id.clone(),
            self.now,
        );
        graft.next_ordinal = self.tree.next_ordinal;
        graft.active_leaf = if leaf_inside {
            self.tree.active_leaf.clone()
        } else {
            Some(node.to_string())
        };
        let mut moved = 0u64;
        for id in &ids {
            if let Some(mut moved_node) = self.tree.nodes.remove(id) {
                // The top of the moved subtree pointed at the node it hung
                // under, which stayed behind. Re-rooting it on the graft's own
                // root is what makes the graft a valid tree of its own.
                if id == node {
                    moved_node.parent = graft.root.clone();
                }
                graft.nodes.insert(id.clone(), moved_node);
                moved += 1;
            }
        }
        if leaf_inside {
            self.tree.active_leaf = Some(parent.clone());
        }

        self.tree.pool.push(PoolEntry {
            graft_id: graft.id.clone(),
            label,
            node_count: moved,
            detached_at: self.now,
            origin_node: node.to_string(),
        });

        let version = self.bump();
        self.record("detach", Some(node.to_string()), ids);
        let restale = self.refold_children(&parent, version);
        let outcome = self.outcome(version, restale);
        Ok(DetachOutcome { outcome, graft })
    }

    /// Graft a detached subtree back onto `new_parent`.
    ///
    /// The graft document is consumed: its nodes move into this tree and keep
    /// their ids. Whether the moved answers stay fresh is decided by the digest
    /// alone — carrying the prefix keeps them fresh, a different prefix makes
    /// them dirty.
    pub fn attach(
        &mut self,
        mut graft: ConversationTree,
        new_parent: &str,
        mode: AttachMode,
        replace: Option<&str>,
    ) -> Result<AttachOutcome, TreeError> {
        if self.tree.node(new_parent).is_none() {
            return Err(TreeError::MissingNode(new_parent.to_string()));
        }

        let mut replaced = None;
        if mode == AttachMode::Replace {
            let victim = replace.ok_or_else(|| TreeError::MissingNode(new_parent.to_string()))?;
            if invariants::would_cycle(self.tree, victim, new_parent) {
                return Err(TreeError::Cycle(victim.to_string()));
            }
            let detached = self.detach(victim, true, None, wf_common::generate_id())?;
            replaced = Some(detached.graft);
        }

        let ids = graft.subtree_ids(&graft.root);
        if self.tree.nodes.len() + ids.len() > MAX_TREE_NODES {
            return Err(TreeError::TooManyNodes {
                count: self.tree.nodes.len() + ids.len(),
                limit: MAX_TREE_NODES,
            });
        }
        let parent_depth = self
            .tree
            .path_to(new_parent)
            .map(|chain| chain.len())
            .unwrap_or(0);
        let graft_depth = invariants::subtree_depth(&graft, &graft.root);
        if parent_depth + graft_depth > MAX_TREE_DEPTH {
            return Err(TreeError::TooDeep {
                depth: parent_depth + graft_depth,
                limit: MAX_TREE_DEPTH,
            });
        }

        let graft_id = graft.id.clone();
        let tops = graft.child_ids(&graft.root);
        for top in &tops {
            let ordinal = self.tree.take_ordinal();
            if let Some(node) = graft.node_mut(top) {
                node.parent = new_parent.to_string();
                node.ordinal = ordinal;
                node.updated_at = self.now;
            }
        }
        let mut moved = 0usize;
        for id in &ids {
            if let Some(node) = graft.nodes.remove(id) {
                self.tree.nodes.insert(id.clone(), node);
                moved += 1;
            }
        }
        self.tree.pool.retain(|entry| entry.graft_id != graft_id);

        let version = self.bump();
        self.record("attach", Some(new_parent.to_string()), ids.clone());
        let mut restale = Vec::new();
        for top in &tops {
            restale.extend(self.refold_node_and_children(top, version));
        }
        Ok(AttachOutcome {
            outcome: self.outcome(version, restale),
            replaced,
            moved,
        })
    }

    // ── content ────────────────────────────────────────────────────────────

    /// Rewrite a node's question in place. The subtree stays; every answer
    /// below it goes stale, which is the whole point of the in-place mode.
    pub fn set_user_content(
        &mut self,
        node: &str,
        user: MessageRef,
    ) -> Result<SurgeryOutcome, TreeError> {
        let target = self
            .tree
            .node_mut(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let previous = std::mem::replace(&mut target.user, user);
        target.superseded_user.push(previous);
        target.updated_at = self.now;

        let version = self.bump();
        self.record(
            "set_content",
            Some(node.to_string()),
            vec![node.to_string()],
        );
        let restale = self.refold_node_and_children(node, version);
        Ok(self.outcome(version, restale))
    }

    /// Replace the answer body with a human-written one: it becomes the new
    /// baseline, and the previous body is archived.
    pub fn set_reply_body(
        &mut self,
        node: &str,
        message: MessageRef,
    ) -> Result<SurgeryOutcome, TreeError> {
        let base = self
            .digests
            .base(node)
            .map(str::to_string)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let version = self.tree.version + 1;

        let target = self
            .tree
            .node_mut(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let reply = target
            .reply
            .as_mut()
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let previous = std::mem::replace(&mut reply.message, message);
        reply.superseded.push(previous);
        reply.state = ReplyState::Kept;
        reply.origin = ReplyOrigin::Human;
        reply.edited_at = Some(self.now);
        reply.acknowledged = None;
        reply.based_on = base;
        reply.based_on_version = version;
        target.updated_at = self.now;

        self.bump();
        self.record("set_reply", Some(node.to_string()), vec![node.to_string()]);
        let restale = self.refold_node_and_children(node, version);
        Ok(self.outcome(version, restale))
    }

    /// Keep or unkeep an answer. Flipping it changes what the children fold on,
    /// so they go stale either way.
    pub fn set_reply_state(
        &mut self,
        node: &str,
        state: ReplyState,
    ) -> Result<SurgeryOutcome, TreeError> {
        let target = self
            .tree
            .node_mut(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let reply = target
            .reply
            .as_mut()
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        reply.state = state;
        reply.acknowledged = None;
        target.updated_at = self.now;

        let version = self.bump();
        self.record(
            "set_reply_state",
            Some(node.to_string()),
            vec![node.to_string()],
        );
        let restale = self.refold_node_and_children(node, version);
        Ok(self.outcome(version, restale))
    }

    /// Start a generation for this node: the current body is archived and a
    /// pending placeholder takes its place.
    pub fn begin_regeneration(
        &mut self,
        node: &str,
        params: Option<ParamsFingerprint>,
    ) -> Result<SurgeryOutcome, TreeError> {
        let now = self.now;
        let target = self
            .tree
            .node_mut(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let placeholder = target
            .reply
            .as_ref()
            .map(|reply| reply.message.clone())
            .unwrap_or_else(|| target.user.clone());
        let previous = target.reply.take();
        let mut pending = Reply::pending(placeholder, params, now);
        if let Some(previous) = previous {
            if previous.state != ReplyState::Superseded {
                pending.superseded.push(previous.message);
            }
            pending.superseded.extend(previous.superseded);
        }
        target.reply = Some(pending);
        target.updated_at = now;

        let version = self.bump();
        self.record("regenerate", Some(node.to_string()), vec![node.to_string()]);
        let restale = self.refold_node_and_children(node, version);
        Ok(self.outcome(version, restale))
    }

    /// Record the outcome of a generation.
    pub fn finish_reply(
        &mut self,
        node: &str,
        message: MessageRef,
        trace: Vec<MessageRef>,
        state: ReplyState,
        params: Option<ParamsFingerprint>,
    ) -> Result<SurgeryOutcome, TreeError> {
        let base = self
            .digests
            .base(node)
            .map(str::to_string)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let version = self.tree.version + 1;
        let now = self.now;

        let target = self
            .tree
            .node_mut(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let reply = target
            .reply
            .get_or_insert_with(|| Reply::pending(message.clone(), params.clone(), now));
        reply.message = message;
        reply.trace = trace;
        reply.state = state;
        reply.origin = ReplyOrigin::Model;
        reply.based_on = base;
        reply.based_on_version = version;
        reply.generated_at = now;
        if params.is_some() {
            reply.params = params;
        }
        target.updated_at = now;

        self.bump();
        self.record(
            "finish_reply",
            Some(node.to_string()),
            vec![node.to_string()],
        );
        let restale = self.refold_node_and_children(node, version);
        Ok(self.outcome(version, restale))
    }

    /// Accept the current staleness of an answer without regenerating it.
    pub fn acknowledge(&mut self, node: &str) -> Result<SurgeryOutcome, TreeError> {
        let Some(base) = self.digests.base(node).map(str::to_string) else {
            return Err(TreeError::MissingNode(node.to_string()));
        };
        let now = self.now;
        let target = self
            .tree
            .node_mut(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let reply = target
            .reply
            .as_mut()
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        reply.acknowledged = Some(base);
        target.updated_at = now;
        let version = self.bump();
        self.record(
            "acknowledge",
            Some(node.to_string()),
            vec![node.to_string()],
        );
        Ok(self.outcome(version, Vec::new()))
    }

    /// Append a freshly generated turn under `parent` and make it the head.
    pub fn append_turn(
        &mut self,
        node: ConversationNode,
        parent: &str,
    ) -> Result<SurgeryOutcome, TreeError> {
        if parent != self.tree.root && self.tree.node(parent).is_none() {
            return Err(TreeError::MissingNode(parent.to_string()));
        }
        if self.tree.nodes.len() + 1 > MAX_TREE_NODES {
            return Err(TreeError::TooManyNodes {
                count: self.tree.nodes.len() + 1,
                limit: MAX_TREE_NODES,
            });
        }
        let ordinal = self.tree.take_ordinal();
        let mut node = node;
        node.parent = parent.to_string();
        node.ordinal = ordinal;
        node.updated_at = self.now;
        let id = node.id.clone();
        self.tree.nodes.insert(id.clone(), node);
        self.tree.active_leaf = Some(id.clone());

        let version = self.bump();
        self.record("append_turn", Some(parent.to_string()), vec![id.clone()]);
        let restale = self.refold_node_and_children(&id, version);
        Ok(self.outcome(version, restale))
    }

    /// Soft-delete a node and everything under it.
    ///
    /// Soft, not physical: the node stays in the document and its messages stay
    /// in the store, but the fold treats it as transparent, which is exactly
    /// what marks every answer below it stale. Restoring is therefore a matter
    /// of clearing `deleted_at` — no content has to be rebuilt.
    pub fn delete_subtree(&mut self, node: &str) -> Result<SurgeryOutcome, TreeError> {
        if node == self.tree.root {
            return Err(TreeError::MissingNode(node.to_string()));
        }
        let target = self
            .tree
            .node(node)
            .ok_or_else(|| TreeError::MissingNode(node.to_string()))?;
        let parent = target.parent.clone();
        let ids = self.tree.subtree_ids(node);
        let now = self.now;
        let leaf_inside = self
            .tree
            .active_leaf
            .as_deref()
            .is_some_and(|leaf| ids.iter().any(|id| id == leaf));
        for id in &ids {
            if let Some(target) = self.tree.nodes.get_mut(id) {
                target.deleted_at.get_or_insert(now);
                target.updated_at = now;
            }
        }
        if leaf_inside {
            self.tree.active_leaf = Some(parent.clone());
        }

        let version = self.bump();
        self.record("delete_subtree", Some(node.to_string()), ids);
        let restale = self.refold_children(&parent, version);
        Ok(self.outcome(version, restale))
    }

    pub fn set_active_leaf(&mut self, node: &str) -> Result<u64, TreeError> {
        if node != self.tree.root && self.tree.node(node).is_none() {
            return Err(TreeError::MissingNode(node.to_string()));
        }
        self.tree.active_leaf = Some(node.to_string());
        Ok(self.bump())
    }

    // ── helpers ────────────────────────────────────────────────────────────

    fn bump(&mut self) -> u64 {
        self.tree.version += 1;
        self.tree.updated_at = self.now;
        self.tree.version
    }

    fn record(&mut self, op: &str, target: Option<NodeId>, nodes: Vec<NodeId>) {
        let version_after = self.tree.version;
        self.tree.ops.push(OpRecord {
            op: op.to_string(),
            target,
            nodes,
            version_before: version_after.saturating_sub(1),
            version_after,
            at: self.now,
        });
        if self.tree.ops.len() > CONVERSATION_MAX_OP_RECORDS {
            let excess = self.tree.ops.len() - CONVERSATION_MAX_OP_RECORDS;
            self.tree.ops.drain(..excess);
        }
    }

    fn outcome(&self, version: u64, restale: Vec<NodeId>) -> SurgeryOutcome {
        SurgeryOutcome {
            version,
            restale,
            dirty: digest::dirty_nodes(self.tree, self.digests).len(),
        }
    }

    /// Recompute `parent`'s children and stamp the ones that moved.
    fn refold_children(&mut self, parent: &str, version: u64) -> Vec<NodeId> {
        let changed = digest::refold(self.tree, self.digests, parent);
        self.restamp(&changed, version);
        changed
    }

    /// Recompute one node and everything below it.
    ///
    /// A content change on the node itself (question, answer, reply state) has
    /// to refresh the node first: its `out` digest is what the children fold
    /// on, so touching only the children would swallow the change.
    fn refold_node_and_children(&mut self, node: &str, version: u64) -> Vec<NodeId> {
        // A missing parent digest means the index never saw this part of the
        // tree (a caller appended behind the editor's back, or a graft landed on
        // a node added in the same request). Rebuilding is linear and cheap, and
        // silently skipping would leave a stale answer looking fresh.
        if digest::refold_node(self.tree, self.digests, node).is_none() {
            *self.digests = digest::build(self.tree);
        }
        let changed = digest::refold_node(self.tree, self.digests, node).unwrap_or(false);
        let mut restale = Vec::new();
        if changed {
            restale.push(node.to_string());
        }
        self.restamp(&restale, version);
        restale.extend(self.refold_children(node, version));
        restale
    }

    fn restamp(&mut self, nodes: &[NodeId], version: u64) {
        let now = self.now;
        for id in nodes {
            if let Some(node) = self.tree.node_mut(id) {
                node.base_version = version;
                node.updated_at = now;
            }
        }
    }
}

/// Build the digest index for a graft document, seeded with its carried prefix.
///
/// A graft that carries its ancestor segments therefore re-folds to exactly the
/// value the subtree had in place, which is what keeps it fresh.
pub fn graft_index(graft: &ConversationTree) -> DigestIndex {
    let seed = graft
        .origin
        .as_ref()
        .map(|origin| digest::fold_prefix(&origin.prefix_segments))
        .unwrap_or_else(digest::root_digest);
    digest::build_with_seed(graft, seed)
}

/// Node id of a tree's root, re-exported so callers do not hard-code it.
pub fn root_id() -> &'static str {
    CONVERSATION_ROOT
}

/// `true` when the node can carry an answer at all.
pub fn can_answer(node: &ConversationNode) -> bool {
    node.kind == NodeKind::Turn && !node.is_deleted()
}

/// `true` when the tree is a detached subtree rather than a session tree.
pub fn is_graft(tree: &ConversationTree) -> bool {
    tree.kind == TreeKind::Graft
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_tree::digest::{self, build};
    use crate::conversation_tree::test_support::{answer, append, new_index, reference, tree};
    use crate::conversation_tree::validate_tree;

    fn chain() -> (ConversationTree, DigestIndex, String, String) {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let b = append(&mut tree, &a, "n2", "q2");
        let mut index = new_index(&tree);
        answer(&mut tree, &mut index, &a, "a1");
        answer(&mut tree, &mut index, &b, "a2");
        (tree, index, a, b)
    }

    #[test]
    fn insert_then_splice_restores_every_digest() {
        let (mut tree, mut index, a, b) = chain();
        let base_before = index.base(&b).expect("base").to_string();
        let out_before = index.out(&b).expect("out").to_string();

        let premise = ConversationNode::premise("p1".to_string(), "root".to_string(), 0, reference("use vim"), 1);
        let outcome = TreeEditor::new(&mut tree, &mut index, 1)
            .insert_before(&a, premise)
            .expect("insert");
        assert!(outcome.restale.contains(&a));
        assert_eq!(
            outcome.dirty, 2,
            "both answers are stale under the new premise"
        );
        assert_ne!(index.base(&b).expect("base"), base_before);

        let outcome = TreeEditor::new(&mut tree, &mut index, 1)
            .splice_out("p1")
            .expect("splice");
        assert!(outcome.restale.contains(&a));
        assert_eq!(outcome.dirty, 0, "removing the premise restores freshness");
        assert_eq!(index.base(&b).expect("base"), base_before);
        assert_eq!(index.out(&b).expect("out"), out_before);
        validate_tree(&tree).expect("valid tree");
    }

    #[test]
    fn detach_with_prefix_keeps_the_subtree_fresh() {
        let (mut tree, mut index, a, _b) = chain();
        let detached = TreeEditor::new(&mut tree, &mut index, 1)
            .detach(&a, true, None, "g1")
            .expect("detach");
        assert_eq!(detached.graft.id, "g1");
        assert_eq!(detached.graft.node_count(), 2);

        let graft = graft_index(&detached.graft);
        let moved = detached.graft.nodes.values().collect::<Vec<_>>();
        assert!(
            moved.iter().all(|node| !graft.is_dirty(node)),
            "carrying the prefix must keep every moved answer fresh"
        );
    }

    #[test]
    fn detach_without_prefix_marks_the_subtree_dirty() {
        // `b` sits under `a`, whose answer is part of the history `b` was
        // generated against; dropping the prefix therefore has to mark it.
        let (mut tree, mut index, _a, b) = chain();
        let detached = TreeEditor::new(&mut tree, &mut index, 1)
            .detach(&b, false, None, "g1")
            .expect("detach");
        let graft = graft_index(&detached.graft);
        let stale = digest::dirty_nodes(&detached.graft, &graft);
        assert_eq!(stale.len(), 1, "the answer lost the history above it");
        assert_eq!(stale[0], b);
    }

    #[test]
    fn attaching_under_another_parent_marks_the_subtree_dirty() {
        let (mut tree, mut index, a, b) = chain();
        let detached = TreeEditor::new(&mut tree, &mut index, 1)
            .detach(&b, true, None, "g1")
            .expect("detach");
        assert!(tree.node(&b).is_none());

        // Link the detached tail under a brand-new branch: its prefix differs,
        // so the answer can no longer be trusted.
        let other = append(&mut tree, "root", "n9", "unrelated question");
        let mut editor = TreeEditor::new(&mut tree, &mut index, 1);
        let outcome = editor
            .attach(detached.graft, &other, AttachMode::Link, None)
            .expect("attach");
        assert_eq!(outcome.moved, 1);
        // Two stale answers: the moved one lost its prefix, and the new parent
        // is itself waiting for an answer.
        assert_eq!(outcome.outcome.dirty, 2);
        assert!(tree.node(&b).is_some());
        validate_tree(&tree).expect("valid tree");
        let _ = a;
    }

    #[test]
    fn rewriting_an_answer_marks_the_children_dirty() {
        let (mut tree, mut index, _a, b) = chain();
        TreeEditor::new(&mut tree, &mut index, 1)
            .set_reply_body("n1", reference("human rewrite"))
            .expect("rewrite");
        assert_eq!(digest::dirty_nodes(&tree, &index), vec![b]);
        assert_eq!(
            tree.node("n1")
                .expect("node")
                .reply
                .as_ref()
                .expect("reply")
                .origin,
            ReplyOrigin::Human
        );
    }

    #[test]
    fn reparent_rejects_cycles() {
        let (mut tree, mut index, a, b) = chain();
        let error = TreeEditor::new(&mut tree, &mut index, 1)
            .reparent(&a, &b)
            .expect_err("cycle must be rejected");
        assert_eq!(error, TreeError::Cycle(a));
    }

    #[test]
    fn splice_out_lifts_children_in_order() {
        let (mut tree, mut index, a, b) = chain();
        TreeEditor::new(&mut tree, &mut index, 1)
            .splice_out(&a)
            .expect("splice");
        assert_eq!(tree.node(&b).expect("b").parent, "root");
        assert!(tree.child_ids(&a).is_empty());
        assert_eq!(tree.active_leaf.as_deref(), Some(b.as_str()));
    }

    #[test]
    fn deleting_a_subtree_takes_it_out_of_the_conversation() {
        let (mut tree, mut index, a, b) = chain();
        assert!(digest::dirty_nodes(&tree, &index).is_empty());

        TreeEditor::new(&mut tree, &mut index, 1)
            .delete_subtree(&a)
            .expect("delete");

        // Both nodes left the conversation, so nothing below is left to go
        // stale; the documents stay so the messages can be inspected.
        assert!(digest::dirty_nodes(&tree, &index).is_empty());
        assert!(tree.node(&a).expect("kept").is_deleted());
        assert!(tree.node(&b).expect("kept").is_deleted());
        validate_tree(&tree).expect("valid tree");
    }

    #[test]
    fn splicing_out_a_node_marks_its_child_stale() {
        let (mut tree, mut index, a, b) = chain();
        TreeEditor::new(&mut tree, &mut index, 1)
            .splice_out(&a)
            .expect("splice");
        let dirty = digest::dirty_nodes(&tree, &index);
        assert!(
            dirty.contains(&b),
            "the child lost its predecessor: {dirty:?}"
        );
    }

    #[test]
    fn a_turn_without_an_answer_counts_as_waiting() {
        let mut tree = tree();
        let node = append(&mut tree, "root", "n1", "q1");
        let index = new_index(&tree);
        let target = tree.node(&node).expect("node");
        assert!(index.is_dirty(target));
    }

    #[test]
    fn regeneration_archives_the_previous_body() {
        let (mut tree, mut index, _a, _b) = chain();
        TreeEditor::new(&mut tree, &mut index, 1)
            .begin_regeneration("n1", None)
            .expect("begin");
        let reply = tree
            .node("n1")
            .expect("node")
            .reply
            .as_ref()
            .expect("reply");
        assert_eq!(reply.state, ReplyState::Pending);
        assert_eq!(reply.superseded.len(), 1);

        TreeEditor::new(&mut tree, &mut index, 1)
            .finish_reply("n1", reference("a1-v2"), Vec::new(), ReplyState::Kept, None)
            .expect("finish");
        let reply = tree
            .node("n1")
            .expect("node")
            .reply
            .as_ref()
            .expect("reply");
        assert_eq!(reply.state, ReplyState::Kept);
        assert_eq!(reply.based_on, index.base("n1").expect("base"));
        let _ = build(&tree);
    }
}
