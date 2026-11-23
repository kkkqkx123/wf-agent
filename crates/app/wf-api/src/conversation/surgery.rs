//! Structural and content operations on a session tree.
//!
//! Five structural verbs cover everything the tree needs:
//!
//! * `insert_before` — add a predecessor (a premise, or a turn that still needs
//!   an answer) between a node and its parent;
//! * `splice_out` — remove a node, lifting its children into its place;
//! * `reparent` — move a node (with its subtree) under another parent;
//! * `detach` — cut a subtree out into a graft in the session's pool;
//! * `attach` — graft a pooled subtree under a parent, optionally replacing the
//!   predecessor that is there.
//!
//! Content edits are separate on purpose, because their consequences differ:
//! rewriting a message **in place** keeps the shape and marks everything below
//! stale (that is the point — an immature premise must stop poisoning the
//! conversation), while **branching** adds a sibling and leaves the old branch
//! byte-identical, so nothing goes stale.
//!
//! Every mutation takes the session's write lock, refuses to run while a turn is
//! generating, checks `if_version`, persists the document and publishes both a
//! shape event and a dirty event.

use wf_execution_shared::conversation_tree::{self, DigestIndex, SurgeryOutcome, TreeEditor};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::message::MessageStorageAdapter;
use wf_types::message::MessageRole;
use wf_types::{ConversationNode, ConversationTree, MessageRef, ReplyState};

use super::query::PoolView;
use super::{events, runtime, store, tree_error};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Common fields of a mutation request.
#[derive(Debug, Clone, Default)]
pub struct SurgeryRequest {
    pub session_id: String,
    pub if_version: Option<u64>,
}

/// What a mutation did to the tree.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct NodeChange {
    pub version: u64,
    /// Answers that may now be stale.
    pub dirty: usize,
    /// Nodes whose prefix digest moved.
    pub restale: Vec<String>,
}

/// A node to insert before another one.
#[derive(Debug, Clone)]
pub struct InsertRequest {
    pub text: String,
    /// Insert a pinned premise (input only, never answered) instead of a turn.
    pub premise: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InsertOutcome {
    pub node_id: String,
    #[serde(flatten)]
    pub change: NodeChange,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetachView {
    pub graft_id: String,
    pub nodes: usize,
    #[serde(flatten)]
    pub change: NodeChange,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachView {
    pub moved: usize,
    /// The predecessor that was replaced and now waits in the pool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_graft_id: Option<String>,
    #[serde(flatten)]
    pub change: NodeChange,
}

fn change_of(outcome: &SurgeryOutcome) -> NodeChange {
    NodeChange {
        version: outcome.version,
        dirty: outcome.dirty,
        restale: outcome.restale.clone(),
    }
}

/// Documents a mutation writes besides the session tree.
#[derive(Default)]
struct SideWrites {
    trees: Vec<ConversationTree>,
    deletes: Vec<String>,
}

/// Run one mutation under the session lock.
async fn mutate<T, F>(
    ctx: &ApiContext,
    session_id: &str,
    if_version: Option<u64>,
    change: &str,
    node: Option<&str>,
    body: F,
) -> ApiResult<(T, SurgeryOutcome)>
where
    F: FnOnce(&mut ConversationTree, &mut DigestIndex) -> ApiResult<(T, SurgeryOutcome)>,
{
    mutate_with_side(
        ctx,
        session_id,
        if_version,
        change,
        node,
        move |tree, digests| {
            let (value, outcome) = body(tree, digests)?;
            Ok((value, outcome, SideWrites::default()))
        },
    )
    .await
}

/// Mutation with side documents. Grafts are saved before the main tree so a
/// failed side write cannot leave a pool entry pointing at a missing document.
async fn mutate_with_side<T, F>(
    ctx: &ApiContext,
    session_id: &str,
    if_version: Option<u64>,
    change: &str,
    node: Option<&str>,
    body: F,
) -> ApiResult<(T, SurgeryOutcome)>
where
    F: FnOnce(
        &mut ConversationTree,
        &mut DigestIndex,
    ) -> ApiResult<(T, SurgeryOutcome, SideWrites)>,
{
    super::require_chat_session(ctx, session_id).await?;
    let runtime = runtime::runtime_for(session_id);
    let _guard = runtime.write().await;
    if runtime.inflight().is_some() {
        return Err(ApiError::Conflict(
            "session is generating a turn; wait for it to settle".into(),
        ));
    }
    let mut tree = store::load_or_adopt(ctx, session_id).await?;
    if let Some(expected) = if_version {
        if tree.version != expected {
            return Err(ApiError::Conflict(format!(
                "tree is at version {}, not {expected}",
                tree.version
            )));
        }
    }
    let mut digests = store::digests(&tree);
    let (value, outcome, side) = body(&mut tree, &mut digests)?;
    for side_tree in &side.trees {
        store::save_tree(ctx, side_tree).await?;
    }
    store::save_tree(ctx, &tree).await?;
    for id in &side.deletes {
        if let Err(error) = ctx.storage.conversation_tree.delete(id).await {
            tracing::warn!(
                target: "wf_api::conversation",
                document_id = %id,
                error = %error,
                "failed to remove a consumed tree document"
            );
        }
    }
    let dirty = conversation_tree::evaluate_dirty(&tree, &digests, None);
    events::tree_changed(ctx, session_id, outcome.version, change, node);
    events::dirty_changed(ctx, session_id, outcome.version, &dirty);
    Ok((value, outcome))
}

/// Insert a node between `target` and its parent.
///
/// The inserted turn has no answer yet, which the dirty list reports as
/// "waiting": stepping the path produces it.
pub async fn insert_before(
    ctx: &ApiContext,
    request: SurgeryRequest,
    target: &str,
    insert: InsertRequest,
) -> ApiResult<InsertOutcome> {
    let text = insert.text.trim().to_string();
    if text.is_empty() {
        return Err(ApiError::Validation(
            "inserted node requires a message".into(),
        ));
    }
    let message =
        super::turn::new_text_message(ctx, &request.session_id, MessageRole::User, &text).await?;
    let reference = MessageRef::new(
        message.id.clone(),
        conversation_tree::message_hash(&message),
    );
    let node_id = wf_common::generate_id();
    let now = wf_common::now();
    let mut node = if insert.premise {
        ConversationNode::premise(node_id.clone(), String::new(), 0, reference, now)
    } else {
        // A turn without a reply: `is_dirty` reports it as waiting, and the fold
        // of everything below it changes the moment it lands.
        ConversationNode::turn(node_id.clone(), String::new(), 0, reference, now)
    };
    node.label = insert.label;

    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "insert_before",
        Some(target),
        move |tree, digests| {
            let mut editor = TreeEditor::new(tree, digests, now);
            let outcome = editor.insert_before(target, node).map_err(tree_error)?;
            Ok(((), outcome))
        },
    )
    .await?;
    // Recompute what the editor reported so the response carries the real
    // staleness count instead of a placeholder.
    let tree = store::require_tree(ctx, &request.session_id).await?;
    let digests = store::digests(&tree);
    let dirty = conversation_tree::evaluate_dirty(&tree, &digests, None);
    Ok(InsertOutcome {
        node_id,
        change: NodeChange {
            version: outcome.version,
            dirty: dirty.entries.len(),
            restale: outcome.restale,
        },
    })
}

/// Remove a node, lifting its children into its place.
pub async fn splice_out(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
) -> ApiResult<NodeChange> {
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "splice_out",
        Some(node),
        |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor.splice_out(node).map_err(tree_error).map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Move a node (with its subtree) under another parent.
pub async fn reparent(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
    new_parent: &str,
) -> ApiResult<NodeChange> {
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "reparent",
        Some(node),
        |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor
                .reparent(node, new_parent)
                .map_err(tree_error)
                .map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Rewrite a node's question **in place**.
///
/// The shape is untouched, the old text is archived on the node, and every
/// answer below it goes stale — which is exactly the intent: the history the
/// answers were produced against no longer exists.
pub async fn write_user_content(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
    text: &str,
) -> ApiResult<NodeChange> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(ApiError::Validation("message must not be blank".into()));
    }
    let message =
        super::turn::new_text_message(ctx, &request.session_id, MessageRole::User, &text).await?;
    let reference = MessageRef::new(
        message.id.clone(),
        conversation_tree::message_hash(&message),
    );
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "set_content",
        Some(node),
        move |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor
                .set_user_content(node, reference)
                .map_err(tree_error)
                .map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Rewrite a node's question as a **new sibling branch**.
///
/// The original branch keeps every byte, so nothing below it changes state: two
/// phrasings can be compared side by side.
pub async fn edit_branch(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
    text: &str,
) -> ApiResult<InsertOutcome> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(ApiError::Validation("message must not be blank".into()));
    }
    let tree = store::load_or_adopt(ctx, &request.session_id).await?;
    let parent = tree
        .node(node)
        .ok_or_else(|| ApiError::not_found("conversation node", node))?
        .parent
        .clone();
    let source = tree
        .node(node)
        .ok_or_else(|| ApiError::not_found("conversation node", node))?
        .clone();

    let message =
        super::turn::new_text_message(ctx, &request.session_id, MessageRole::User, &text).await?;
    let reference = MessageRef::new(
        message.id.clone(),
        conversation_tree::message_hash(&message),
    );
    let node_id = wf_common::generate_id();
    let now = wf_common::now();
    let mut fresh = ConversationNode::turn(node_id.clone(), parent.clone(), 0, reference, now);
    fresh.kind = source.kind;
    fresh.label = source.label.clone();

    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "edit_branch",
        Some(&node_id),
        move |tree, digests| {
            // The new branch becomes the head, which is what "try this instead"
            // means: the old branch stays reachable but is no longer sent.
            let mut editor = TreeEditor::new(tree, digests, now);
            let outcome = editor.append_turn(fresh, &parent).map_err(tree_error)?;
            Ok(((), outcome))
        },
    )
    .await?;
    let tree = store::require_tree(ctx, &request.session_id).await?;
    let digests = store::digests(&tree);
    let dirty = conversation_tree::evaluate_dirty(&tree, &digests, None);
    Ok(InsertOutcome {
        node_id,
        change: NodeChange {
            version: outcome.version,
            dirty: dirty.entries.len(),
            restale: outcome.restale,
        },
    })
}

/// Replace an answer with a human-written body.
///
/// The old body is archived on the reply, `origin` becomes `Human` so a later
/// regeneration does not silently overwrite the edit, and the answers below go
/// stale because their history changed.
pub async fn write_reply_body(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
    text: &str,
) -> ApiResult<NodeChange> {
    if text.trim().is_empty() {
        return Err(ApiError::Validation("reply must not be blank".into()));
    }
    let message =
        super::turn::new_text_message(ctx, &request.session_id, MessageRole::Assistant, text)
            .await?;
    let reference = MessageRef::new(
        message.id.clone(),
        conversation_tree::message_hash(&message),
    );
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "set_reply_body",
        Some(node),
        move |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor
                .set_reply_body(node, reference)
                .map_err(tree_error)
                .map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Drop an answer from the context without deleting it.
pub async fn discard_reply(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
) -> ApiResult<NodeChange> {
    set_state(ctx, request, node, ReplyState::Discarded).await
}

/// Put a discarded answer back into the context.
pub async fn restore_reply(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
) -> ApiResult<NodeChange> {
    set_state(ctx, request, node, ReplyState::Kept).await
}

async fn set_state(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
    state: ReplyState,
) -> ApiResult<NodeChange> {
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "reply_state",
        Some(node),
        move |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor
                .set_reply_state(node, state)
                .map_err(tree_error)
                .map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Accept a stale answer as it is, without regenerating it.
pub async fn acknowledge_reply(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
) -> ApiResult<NodeChange> {
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "acknowledge",
        Some(node),
        |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor
                .acknowledge(node)
                .map_err(tree_error)
                .map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Cut a subtree out into the session's graft pool.
///
/// `carry_prefix` copies the ancestors into the graft so the subtree keeps its
/// freshness; without it the grafted answers are honestly marked stale, because
/// the history above them is gone.
pub async fn detach_subtree(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
    carry_prefix: bool,
    label: Option<String>,
) -> ApiResult<DetachView> {
    let graft_id = wf_common::generate_id();
    let graft_id_for_edit = graft_id.clone();
    let (nodes, outcome) = mutate_with_side(
        ctx,
        &request.session_id,
        request.if_version,
        "detach",
        Some(node),
        move |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            let detached = editor
                .detach(node, carry_prefix, label, graft_id_for_edit)
                .map_err(tree_error)?;
            let nodes = detached.graft.nodes.len();
            Ok((
                nodes,
                detached.outcome,
                SideWrites {
                    trees: vec![detached.graft],
                    deletes: Vec::new(),
                },
            ))
        },
    )
    .await?;
    Ok(DetachView {
        graft_id,
        nodes,
        change: change_of(&outcome),
    })
}

/// Graft a pooled subtree under `parent`.
pub async fn attach_graft(
    ctx: &ApiContext,
    request: SurgeryRequest,
    graft_id: &str,
    parent: &str,
    replace: Option<&str>,
) -> ApiResult<AttachView> {
    let graft = store::load_graft(ctx, graft_id).await?;
    let graft_source_session = graft.session_id.clone();
    let cross_session = graft_source_session != request.session_id;
    if cross_session {
        if let Ok(Some(target)) = store::load_existing(ctx, &request.session_id).await {
            if target.profile_id != graft.profile_id {
                tracing::warn!(
                    target: "wf_api::conversation",
                    graft_id,
                    "grafted subtree comes from a different profile"
                );
            }
        }
    }
    let parent_owned = parent.to_string();
    let replace_owned = replace.map(str::to_string);
    let graft_id_for_delete = graft_id.to_string();
    let (moved, outcome) = mutate_with_side(
        ctx,
        &request.session_id,
        request.if_version,
        "attach",
        Some(parent),
        move |tree, digests| {
            let now = wf_common::now();
            let mode = if replace_owned.is_some() {
                conversation_tree::AttachMode::Replace
            } else {
                conversation_tree::AttachMode::Link
            };
            let mut editor = TreeEditor::new(tree, digests, now);
            let attached = editor
                .attach(graft, &parent_owned, mode, replace_owned.as_deref())
                .map_err(tree_error)?;
            let replaced_graft_id = attached.replaced.as_ref().map(|tree| tree.id.clone());
            Ok((
                (attached.moved, replaced_graft_id),
                attached.outcome,
                SideWrites {
                    trees: attached.replaced.into_iter().collect(),
                    deletes: vec![graft_id_for_delete],
                },
            ))
        },
    )
    .await?;
    let (moved, replaced_graft_id) = moved;
    if cross_session {
        store::remove_pool_entry(ctx, &graft_source_session, graft_id).await?;
    }
    Ok(AttachView {
        moved,
        replaced_graft_id,
        change: change_of(&outcome),
    })
}

/// Grafts waiting in the session's pool.
pub async fn list_grafts(ctx: &ApiContext, session_id: &str) -> ApiResult<Vec<PoolView>> {
    super::require_chat_session(ctx, session_id).await?;
    let tree = store::load_or_adopt(ctx, session_id).await?;
    Ok(tree
        .pool
        .iter()
        .map(|entry| PoolView {
            graft_id: entry.graft_id.clone(),
            label: entry.label.clone(),
            node_count: entry.node_count,
            detached_at: entry.detached_at,
        })
        .collect())
}

/// Move the session's head.
pub async fn set_active_leaf(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
) -> ApiResult<u64> {
    let (version, _) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "set_leaf",
        Some(node),
        |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            let version = editor.set_active_leaf(node).map_err(tree_error)?;
            Ok((
                version,
                SurgeryOutcome {
                    version,
                    ..SurgeryOutcome::default()
                },
            ))
        },
    )
    .await?;
    Ok(version)
}

/// Soft-delete a node and its subtree.
pub async fn delete_node(
    ctx: &ApiContext,
    request: SurgeryRequest,
    node: &str,
) -> ApiResult<NodeChange> {
    let (_, outcome) = mutate(
        ctx,
        &request.session_id,
        request.if_version,
        "delete_subtree",
        Some(node),
        |tree, digests| {
            let now = wf_common::now();
            let mut editor = TreeEditor::new(tree, digests, now);
            editor
                .delete_subtree(node)
                .map_err(tree_error)
                .map(|o| ((), o))
        },
    )
    .await?;
    Ok(change_of(&outcome))
}

/// Delete a session: every tree document, every pooled graft, and the messages
/// the session accumulated.
pub async fn delete_session(ctx: &ApiContext, session_id: &str) -> ApiResult<()> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    super::require_chat_session(ctx, session_id).await?;
    let runtime = runtime::runtime_for(session_id);
    let _guard = runtime.write().await;
    if let Some(token) = runtime.cancel_token(None) {
        token.cancel();
    }
    store::delete_documents(ctx, session_id).await?;
    let messages = ctx
        .storage
        .message
        .list_by_execution(session_id, None)
        .await
        .map_err(ApiError::from)?;
    for message in messages {
        ctx.storage
            .message
            .delete(&message.id)
            .await
            .map_err(ApiError::from)?;
    }
    let _ = ctx.storage.agent_execution.delete(session_id).await;
    runtime::forget(session_id);
    Ok(())
}
