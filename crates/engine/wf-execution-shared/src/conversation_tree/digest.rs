//! Content digests: the reason "dirty" is computed instead of remembered.
//!
//! Every node contributes one *segment* to a fold. The fold value at a node is
//! the digest of everything a model would have seen before that node's answer,
//! so comparing it with the digest recorded when the answer was generated
//! answers "has the history changed under this answer?".
//!
//! Properties this buys, none of which a boolean `dirty` flag can offer:
//!
//! * **Self-healing**: undoing a change restores the fold value, so the flag
//!   never has to be cleared by hand.
//! * **Local**: no propagation code. A change to an ancestor's digest *is* a
//!   change to every descendant's fold input.
//! * **Graft-transparent**: a detached subtree that carries its ancestor
//!   segments re-folds to the same value, so it stays fresh; one that does not
//!   becomes dirty as a whole, which is the honest answer.
//!
//! Only content enters the fold: node ids, sibling ordinals, timestamps and
//! message ids are excluded so a surgery round trip is digest-neutral.

use std::collections::HashMap;

use wf_types::{ConversationNode, ConversationTree, NodeId};

/// Seed of the virtual root's digest.
pub const ROOT_DIGEST_SEED: &str = "wf.cvtree.v1:root";

/// Hex characters kept from a BLAKE3 digest (128 bits).
const DIGEST_HEX_LEN: usize = 32;

/// Message metadata keys excluded from the hash: they are not replayed, and
/// patching them in later must not look like a history change.
const VOLATILE_METADATA_KEYS: &[&str] = &["usage", "cost", "timing", "step_of"];

/// Digest of the virtual root, i.e. the fold seed of every tree.
pub fn root_digest() -> String {
    hex(blake3::hash(ROOT_DIGEST_SEED.as_bytes()))
}

fn hex(hash: blake3::Hash) -> String {
    let full = hash.to_hex();
    full[..DIGEST_HEX_LEN.min(full.len())].to_string()
}

/// Content hash of one stored message.
///
/// `id` and `timestamp` are dropped so the same content written as a new
/// message hashes identically; volatile metadata is dropped for the same
/// reason. Everything else — role, content, tool calls, thinking signature,
/// the rest of metadata — is hashed, because that is what gets replayed.
pub fn message_hash(message: &wf_types::message::Message) -> String {
    let mut value = match serde_json::to_value(message) {
        Ok(value) => value,
        Err(_) => return String::new(),
    };
    if let Some(object) = value.as_object_mut() {
        object.remove("id");
        object.remove("timestamp");
        if let Some(metadata) = object.get_mut("metadata").and_then(|m| m.as_object_mut()) {
            for key in VOLATILE_METADATA_KEYS {
                metadata.remove(*key);
            }
            if metadata.is_empty() {
                object.remove("metadata");
            }
        }
    }
    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    hex(blake3::hash(&bytes))
}

/// Fold one segment onto a parent digest.
///
/// `reply` is `Some((message_hash, trace_hashes))` when the node's answer
/// contributes to the context, `None` when it does not (no answer, discarded,
/// interrupted, superseded, or a premise).
pub fn fold_segment(
    parent: &str,
    context: &[String],
    user: &str,
    reply: Option<(&str, &[String])>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"seg\x1f");
    hasher.update(parent.as_bytes());
    for hash in context {
        hasher.update(b"\x1fctx\x1f");
        hasher.update(hash.as_bytes());
    }
    hasher.update(b"\x1fuser\x1f");
    hasher.update(user.as_bytes());
    match reply {
        Some((message, trace)) => {
            hasher.update(b"\x1freply\x1f");
            hasher.update(message.as_bytes());
            for hash in trace {
                hasher.update(b"\x1ftrace\x1f");
                hasher.update(hash.as_bytes());
            }
        }
        None => {
            hasher.update(b"\x1freply\x1f-");
        }
    }
    hex(hasher.finalize())
}

/// The three hash parts of a node's segment.
pub type Segment = (Vec<String>, String, Option<(String, Vec<String>)>);

pub fn segment_of(node: &ConversationNode) -> Segment {
    let context: Vec<String> = node.context.iter().map(|m| m.hash.clone()).collect();
    let reply = node.context_reply().map(|reply| {
        (
            reply.message.hash.clone(),
            reply.trace.iter().map(|m| m.hash.clone()).collect(),
        )
    });
    (context, node.user.hash.clone(), reply)
}

/// Digest values for every node of one tree.
///
/// Digest values are never persisted: they are rebuilt from the tree document
/// on load (no message reads, because the tree carries content hashes). Storing
/// them would only add a way for them to drift from the content.
#[derive(Debug, Clone, Default)]
pub struct DigestIndex {
    base: HashMap<NodeId, String>,
    out: HashMap<NodeId, String>,
}

impl DigestIndex {
    /// Digest of everything before the node's own answer: what the answer
    /// should have been generated against.
    pub fn base(&self, node: &str) -> Option<&str> {
        self.base.get(node).map(String::as_str)
    }

    /// Digest of everything including the node's answer: what its children
    /// fold on.
    pub fn out(&self, node: &str) -> Option<&str> {
        self.out.get(node).map(String::as_str)
    }

    pub fn set(&mut self, node: impl Into<NodeId>, base: String, out: String) {
        let node = node.into();
        self.base.insert(node.clone(), base);
        self.out.insert(node, out);
    }

    /// Whether the node's answer was generated against a history that has
    /// since changed.
    pub fn is_dirty(&self, node: &ConversationNode) -> bool {
        if node.is_deleted() {
            return false;
        }
        let Some(reply) = node.reply.as_ref() else {
            // A turn with no answer yet — a predecessor that was just inserted —
            // is waiting for one. A premise never answers, so it never waits.
            return node.kind == wf_types::NodeKind::Turn;
        };
        if !reply.state.recomputable() {
            return false;
        }
        match self.base(&node.id) {
            Some(base) => {
                // An accepted stale answer is still stale in content, but the
                // user chose to keep it. A later upstream change moves `base`
                // again and makes it stale anew.
                reply.based_on != base && reply.acknowledged.as_deref() != Some(base)
            }
            None => false,
        }
    }

    /// Whether the user already accepted this exact staleness.
    pub fn is_acknowledged(&self, node: &ConversationNode) -> bool {
        let Some(reply) = node.reply.as_ref() else {
            return false;
        };
        match (reply.acknowledged.as_deref(), self.base(&node.id)) {
            (Some(acknowledged), Some(base)) => acknowledged == base && reply.based_on != base,
            _ => false,
        }
    }
}

/// Rebuild the index for a whole tree in one pass.
pub fn build(tree: &ConversationTree) -> DigestIndex {
    build_with_seed(tree, root_digest())
}

/// Rebuild the index with an explicit root seed.
///
/// Graft documents use this: their seed is the fold of the carried prefix, so
/// a subtree that kept its ancestors keeps its freshness instead of being
/// marked stale by the move itself.
pub fn build_with_seed(tree: &ConversationTree, seed: String) -> DigestIndex {
    let mut index = DigestIndex::default();
    index.set(tree.root.clone(), seed.clone(), seed);
    refold(tree, &mut index, &tree.root);
    index
}

/// Recompute one node from its parent's current `out` digest.
///
/// Returns whether the node's `base` actually moved. Needed whenever a node's
/// own segment changes (its question, its answer, its reply state): the node's
/// `out` digest feeds its children, so refreshing only the children would leave
/// the change invisible.
pub fn refold_node(tree: &ConversationTree, index: &mut DigestIndex, node: &str) -> Option<bool> {
    let current = tree.node(node)?;
    let parent_out = index.out(&current.parent)?.to_string();
    // A soft-deleted node contributes nothing to the projection, so it must
    // contribute nothing to the fold either: its own segment is transparent and
    // its children fold on the parent's digest. Keeping it in the fold would
    // make the digest describe a history the model never receives.
    if current.is_deleted() {
        let changed = index.base(node) != Some(parent_out.as_str());
        index.set(node.to_string(), parent_out.clone(), parent_out);
        return Some(changed);
    }
    let (context, user, reply) = segment_of(current);
    let base = fold_segment(&parent_out, &context, &user, None);
    let out = fold_segment(
        &parent_out,
        &context,
        &user,
        reply.as_ref().map(|(m, t)| (m.as_str(), t.as_slice())),
    );
    let changed = index.base(node) != Some(base.as_str());
    index.set(node.to_string(), base, out);
    Some(changed)
}

/// Recompute every descendant of `parent`. `parent` itself must be current.
///
/// Returns the ids whose `base` value actually changed, so the caller can stamp
/// `base_version` on exactly those nodes — that stamp is what the "stale since
/// vN" display and the culprit lookup read.
pub fn refold(tree: &ConversationTree, index: &mut DigestIndex, parent: &str) -> Vec<NodeId> {
    let mut changed = Vec::new();
    let mut stack = vec![parent.to_string()];
    while let Some(current) = stack.pop() {
        let Some(parent_out) = index.out(&current).map(str::to_string) else {
            continue;
        };
        for child in tree.children(&current) {
            if child.is_deleted() {
                // Transparent, exactly like `refold_node`: deleting a node
                // removes its contribution from the context, so every answer
                // below it goes stale.
                if index.base(&child.id).is_some_and(|old| old != parent_out) {
                    changed.push(child.id.clone());
                }
                index.set(child.id.clone(), parent_out.clone(), parent_out.clone());
                stack.push(child.id.clone());
                continue;
            }
            let (context, user, reply) = segment_of(child);
            let reply_view = reply
                .as_ref()
                .map(|(message, trace)| (message.as_str(), trace.as_slice()));
            let base = fold_segment(&parent_out, &context, &user, None);
            let out = fold_segment(&parent_out, &context, &user, reply_view);
            if index.base(&child.id).is_some_and(|old| old != base) {
                changed.push(child.id.clone());
            }
            index.set(child.id.clone(), base, out);
            stack.push(child.id.clone());
        }
    }
    changed
}

/// Convenience: ids of every node in the tree whose answer is dirty.
pub fn dirty_nodes(tree: &ConversationTree, index: &DigestIndex) -> Vec<NodeId> {
    tree.nodes
        .values()
        .filter(|node| index.is_dirty(node))
        .map(|node| node.id.clone())
        .collect()
}

/// One folded segment, materialized into a graft document.
pub fn segment_from_node(node: &ConversationNode) -> wf_types::PrefixSegment {
    let (context, user, reply) = segment_of(node);
    wf_types::PrefixSegment {
        context,
        user,
        reply: reply.map(|(message, trace)| wf_types::SegmentReply { message, trace }),
    }
}

/// Fold a materialized prefix from the root seed. Used when a graft tree is
/// loaded: its `in_digest` is the fold of the copied segments, which equals
/// the original parent's `in_digest` when the prefix was carried over.
pub fn fold_prefix(segments: &[wf_types::PrefixSegment]) -> String {
    let mut digest = root_digest();
    for segment in segments {
        let reply = segment
            .reply
            .as_ref()
            .map(|reply| (reply.message.as_str(), reply.trace.as_slice()));
        digest = fold_segment(&digest, &segment.context, &segment.user, reply);
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_tree::test_support::{answer, append, message, new_index, tree};

    #[test]
    fn message_hash_ignores_id_and_timestamp() {
        let mut first = message("same body");
        let mut second = message("same body");
        second.id = "another-id".into();
        second.timestamp = first.timestamp + 5_000;
        assert_eq!(message_hash(&first), message_hash(&second));

        first.content = wf_types::message::Message::user_text("other body".to_string()).content;
        assert_ne!(message_hash(&first), message_hash(&second));
    }

    #[test]
    fn fold_is_order_sensitive() {
        let parent = root_digest();
        let forward = fold_segment(&parent, &["a".into()], "b", None);
        let backward = fold_segment(&parent, &["b".into()], "a", None);
        assert_ne!(forward, backward);
    }

    #[test]
    fn digests_ignore_sibling_ordinals() {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let index = new_index(&tree);
        let base_after_append = index.base(&a).expect("base").to_string();

        // Renumbering siblings must not look like a history change.
        tree.node_mut(&a).expect("node").ordinal = 999;
        let renumbered = new_index(&tree);
        assert_eq!(renumbered.base(&a).expect("base"), base_after_append);
    }

    #[test]
    fn answer_goes_dirty_when_its_question_changes() {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let mut index = new_index(&tree);
        answer(&mut tree, &mut index, &a, "a1");
        assert!(!index.is_dirty(tree.node(&a).expect("node")));

        tree.node_mut(&a).expect("node").user =
            wf_types::MessageRef::new("m-q1-v2", message_hash(&message("q1-v2")));
        refold(&tree, &mut index, "root");
        assert!(index.is_dirty(tree.node(&a).expect("node")));
    }

    #[test]
    fn discarding_a_reply_dirties_its_children() {
        let mut tree = tree();
        let a = append(&mut tree, "root", "n1", "q1");
        let b = append(&mut tree, &a, "n2", "q2");
        let mut index = new_index(&tree);
        answer(&mut tree, &mut index, &a, "a1");
        answer(&mut tree, &mut index, &b, "a2");
        assert_eq!(dirty_nodes(&tree, &index).len(), 0);

        tree.node_mut(&a)
            .expect("node")
            .reply
            .as_mut()
            .expect("reply")
            .state = wf_types::ReplyState::Discarded;
        refold(&tree, &mut index, "root");
        assert_eq!(dirty_nodes(&tree, &index), vec![b]);
    }
}
