//! Shared fixtures for the conversation-tree tests.

use wf_types::{ConversationNode, ConversationTree, MessageRef, Reply, ReplyState};

use super::digest::{self, DigestIndex};

/// A message carrying `text` as its body.
pub fn message(text: &str) -> wf_types::message::Message {
    wf_types::message::Message::user_text(text.to_string())
}

/// A message ref carrying the content hash of `text`.
pub fn reference(text: &str) -> MessageRef {
    let message = message(text);
    MessageRef::new(message.id.clone(), digest::message_hash(&message))
}

/// An empty session tree with a deterministic id.
pub fn tree() -> ConversationTree {
    ConversationTree::new_session("tree-1", "session-1", None, 1)
}

/// The digest index of a freshly built tree.
pub fn new_index(tree: &ConversationTree) -> DigestIndex {
    digest::build(tree)
}

/// Append a turn under `parent` with id `id`.
pub fn append(tree: &mut ConversationTree, parent: &str, id: &str, text: &str) -> String {
    let ordinal = tree.take_ordinal();
    let node = ConversationNode::turn(
        id.to_string(),
        parent.to_string(),
        ordinal,
        reference(text),
        1,
    );
    tree.nodes.insert(id.to_string(), node);
    tree.active_leaf = Some(id.to_string());
    id.to_string()
}

/// Give `id` a kept answer whose `based_on` matches the current history.
pub fn answer(tree: &mut ConversationTree, index: &mut DigestIndex, id: &str, text: &str) {
    let message = reference(text);
    let base = index
        .base(id)
        .map(str::to_string)
        .unwrap_or_else(digest::root_digest);
    {
        let node = tree.node_mut(id).expect("node exists");
        let reply = node
            .reply
            .get_or_insert_with(|| Reply::pending(message.clone(), None, 1));
        reply.message = message;
        reply.trace.clear();
        reply.state = ReplyState::Kept;
        reply.based_on = base;
        reply.based_on_version = 1;
    }
    // The node's own `out` digest now folds the answer, so refresh it before
    // its children: refreshing only the children would read a stale value.
    digest::refold_node(tree, index, id);
    digest::refold(tree, index, id);
}

/// Rewrite a node's question in place, then bring the index up to date.
pub fn rewrite_user(tree: &mut ConversationTree, index: &mut DigestIndex, id: &str, text: &str) {
    let reference = reference(text);
    {
        let node = tree.node_mut(id).expect("node exists");
        let previous = std::mem::replace(&mut node.user, reference);
        node.superseded_user.push(previous);
    }
    // The node's own base changed too, so recompute it before its children.
    digest::refold_node(tree, index, id);
    digest::refold(tree, index, id);
}

/// Number of stale answers in the whole tree.
pub fn dirty_count(tree: &ConversationTree, index: &DigestIndex) -> usize {
    digest::dirty_nodes(tree, index).len()
}
