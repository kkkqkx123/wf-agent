//! Adopting a legacy linear conversation into a tree.
//!
//! Chat sessions stored before the tree existed are a flat message sequence.
//! They are adopted lazily: the tree is synthesized read-only on first access
//! and only written back when a surgery actually needs it, so a read never
//! turns into a write.
//!
//! Adoption marks everything fresh: the answers were generated against exactly
//! this history, so `based_on` is filled from the freshly built digest index.

use wf_types::{ConversationNode, ConversationTree, MessageRef, NodeKind, Reply, ReplyState};

use super::digest::{self, DigestIndex};

/// Build a single-chain tree from linearly ordered messages.
///
/// Grouping rules:
///
/// * a `system` message becomes context of the node it precedes;
/// * a `user` message opens a turn;
/// * `assistant` / `tool` messages after a turn belong to that turn's reply
///   (the tool messages become its trace);
/// * an `assistant` message with no turn to attach to opens its own node, so no
///   stored content is dropped.
pub fn from_linear(
    tree_id: impl Into<String>,
    session_id: impl Into<String>,
    profile_id: Option<String>,
    messages: &[wf_types::message::Message],
    now: wf_types::Timestamp,
) -> ConversationTree {
    use wf_types::message::MessageRole;

    let mut tree = ConversationTree::new_session(tree_id, session_id, profile_id, now);
    let mut parent = tree.root.clone();
    let mut context: Vec<MessageRef> = Vec::new();

    for (position, message) in messages.iter().enumerate() {
        let reference = MessageRef::new(message.id.clone(), digest::message_hash(message));
        match message.role {
            MessageRole::System => context.push(reference),
            MessageRole::User => {
                let id = node_id(position);
                let mut node = ConversationNode::turn(
                    id.clone(),
                    parent.clone(),
                    tree.take_ordinal(),
                    reference,
                    stamp(message, now),
                );
                node.context = std::mem::take(&mut context);
                tree.nodes.insert(id.clone(), node);
                tree.active_leaf = Some(id.clone());
                parent = id;
            }
            MessageRole::Assistant | MessageRole::Tool => {
                let Some(current) = tree.active_leaf.clone() else {
                    // Leading assistant content: keep it as its own node rather
                    // than dropping stored history.
                    let id = node_id(position);
                    let mut node = ConversationNode::turn(
                        id.clone(),
                        parent.clone(),
                        tree.take_ordinal(),
                        reference,
                        stamp(message, now),
                    );
                    node.context = std::mem::take(&mut context);
                    node.kind = NodeKind::Turn;
                    tree.nodes.insert(id.clone(), node);
                    tree.active_leaf = Some(id.clone());
                    parent = id;
                    continue;
                };
                let Some(node) = tree.node_mut(&current) else {
                    continue;
                };
                match message.role {
                    MessageRole::Assistant => {
                        // A second assistant message in a row starts a new node
                        // instead of overwriting the first answer.
                        if let Some(reply) = node.reply.as_mut() {
                            if reply.state == ReplyState::Kept {
                                let new_id = node_id(position);
                                let mut follower = ConversationNode::turn(
                                    new_id.clone(),
                                    current.clone(),
                                    tree.take_ordinal(),
                                    reference,
                                    stamp(message, now),
                                );
                                follower.context = std::mem::take(&mut context);
                                tree.nodes.insert(new_id.clone(), follower);
                                tree.active_leaf = Some(new_id.clone());
                                parent = new_id;
                                continue;
                            }
                        }
                        let reply = node.reply.get_or_insert_with(|| {
                            Reply::pending(reference.clone(), None, stamp(message, now))
                        });
                        reply.message = reference;
                        reply.state = ReplyState::Kept;
                        reply.trace.clear();
                    }
                    MessageRole::Tool => {
                        if let Some(reply) = node.reply.as_mut() {
                            reply.trace.push(reference);
                        } else {
                            let reply = node.reply.insert(Reply::pending(
                                reference,
                                None,
                                stamp(message, now),
                            ));
                            reply.state = ReplyState::Kept;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    mark_fresh(&mut tree, now);
    tree
}

/// Fill `based_on` from a fresh digest index so adopted history starts clean.
fn mark_fresh(tree: &mut ConversationTree, now: wf_types::Timestamp) {
    let index = digest::build(tree);
    let ids: Vec<String> = tree.nodes.keys().cloned().collect();
    for id in ids {
        let Some(base) = index.base(&id).map(str::to_string) else {
            continue;
        };
        if let Some(node) = tree.node_mut(&id) {
            if let Some(reply) = node.reply.as_mut() {
                reply.based_on = base;
                reply.based_on_version = 1;
                reply.generated_at = now;
            }
        }
    }
    tree.updated_at = now;
}

fn node_id(position: usize) -> String {
    format!("n{}", position + 1)
}

fn stamp(
    message: &wf_types::message::Message,
    fallback: wf_types::Timestamp,
) -> wf_types::Timestamp {
    if message.timestamp > 0 {
        message.timestamp
    } else {
        fallback
    }
}

/// Digest index for an adopted tree, exposed so callers do not rebuild it.
pub fn adopted_index(tree: &ConversationTree) -> DigestIndex {
    digest::build(tree)
}
