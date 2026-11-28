//! Persistence and adoption of a session's tree.
//!
//! One session owns exactly one `Session` tree plus zero or more `Graft`
//! documents waiting in its pool. Both live in the same table, keyed by
//! document id, and are filtered by `sessionId`.

use wf_execution_shared::conversation_tree::{self, adopt, DigestIndex, TreeEditor};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::conversation_tree::ConversationTreeStorageAdapter;
use wf_types::{ConversationTree, ReplyState, TreeKind};

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Load the session's tree, adopting the stored linear history when the
/// session predates the tree.
///
/// Adoption is read-only: a session that is only read never gains a tree
/// document, so an untouched conversation costs no write. The first mutating
/// request persists the adopted tree through [`save_tree`].
pub async fn load_or_adopt(ctx: &ApiContext, session_id: &str) -> ApiResult<ConversationTree> {
    if let Some(tree) = load_existing(ctx, session_id).await? {
        return Ok(tree);
    }
    let history = crate::agent::agent_message::conversation_history(ctx, session_id, None).await?;
    let profile_id = ctx
        .storage
        .agent_execution
        .load(session_id)
        .await
        .ok()
        .flatten()
        .and_then(|record| record.context)
        .and_then(|context| context.profile_id);
    Ok(adopt::from_linear(
        wf_common::generate_id(),
        session_id,
        profile_id,
        &history,
        wf_common::now(),
    ))
}

/// Load the tree document without adopting anything. `None` when the session
/// has no tree yet.
pub async fn load_existing(
    ctx: &ApiContext,
    session_id: &str,
) -> ApiResult<Option<ConversationTree>> {
    let mut tree = ctx
        .storage
        .conversation_tree
        .load_session_tree(session_id)
        .await
        .map_err(ApiError::from)?;
    if let Some(tree) = tree.as_mut() {
        // A document that fails the invariants would project into a wrong
        // context, so it is rejected instead of repaired in place.
        conversation_tree::validate_tree(tree)
            .map_err(|error| ApiError::Conflict(format!("tree of '{session_id}': {error}")))?;
        recover_pending(ctx, tree, session_id).await?;
    }
    Ok(tree)
}

/// A `Pending` reply whose driver is gone cannot ever settle. Turning it into
/// an interrupted answer on load keeps the stepper able to regenerate it.
async fn recover_pending(
    ctx: &ApiContext,
    tree: &mut ConversationTree,
    session_id: &str,
) -> ApiResult<()> {
    let pending: Vec<String> = tree
        .nodes
        .values()
        .filter(|node| {
            node.reply
                .as_ref()
                .is_some_and(|reply| reply.state == ReplyState::Pending)
        })
        .map(|node| node.id.clone())
        .collect();
    if pending.is_empty() {
        return Ok(());
    }
    let driver_node = super::runtime::runtime_for(session_id)
        .inflight()
        .map(|inflight| inflight.node_id);
    if pending
        .iter()
        .all(|id| Some(id.as_str()) == driver_node.as_deref())
    {
        return Ok(());
    }
    let now = wf_common::now();
    let mut digests = conversation_tree::build(tree);
    {
        let mut editor = TreeEditor::new(tree, &mut digests, now);
        for id in &pending {
            if Some(id.as_str()) == driver_node.as_deref() {
                continue;
            }
            if tree_node_is_pending(editor.tree(), id) {
                let _ = editor.set_reply_state(id, ReplyState::Interrupted);
            }
        }
    }
    save_tree(ctx, tree).await
}

fn tree_node_is_pending(tree: &ConversationTree, id: &str) -> bool {
    tree.node(id)
        .and_then(|node| node.reply.as_ref())
        .is_some_and(|reply| reply.state == ReplyState::Pending)
}

/// Load a tree and fail when it does not exist.
pub async fn require_tree(ctx: &ApiContext, session_id: &str) -> ApiResult<ConversationTree> {
    load_existing(ctx, session_id)
        .await?
        .ok_or_else(|| ApiError::not_found("conversation tree", session_id))
}

/// Persist the tree document. The caller holds the session's write lock.
pub async fn save_tree(ctx: &ApiContext, tree: &ConversationTree) -> ApiResult<()> {
    ctx.storage
        .conversation_tree
        .save(tree)
        .await
        .map_err(ApiError::from)
}

/// Load a pooled graft document.
pub async fn load_graft(ctx: &ApiContext, graft_id: &str) -> ApiResult<ConversationTree> {
    let tree = ctx
        .storage
        .conversation_tree
        .load(graft_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("graft", graft_id))?;
    if tree.kind != TreeKind::Graft {
        return Err(ApiError::Validation(format!(
            "'{graft_id}' is a session tree, not a graft"
        )));
    }
    conversation_tree::validate_tree(&tree)
        .map_err(|error| ApiError::Conflict(format!("graft '{graft_id}': {error}")))?;
    Ok(tree)
}

/// Create a fresh session: an empty tree plus the `Chat` execution record the
/// rest of the app lists sessions from.
pub async fn create_session(
    ctx: &ApiContext,
    profile_id: Option<String>,
    directory: Option<String>,
    allowed_tools: Option<Vec<String>>,
) -> ApiResult<ConversationTree> {
    let session_id = wf_common::generate_id();
    let now = wf_common::now();
    let tree = ConversationTree::new_session(
        wf_common::generate_id(),
        session_id.clone(),
        profile_id.clone(),
        now,
    );
    save_tree(ctx, &tree).await?;
    record_session(
        ctx,
        &session_id,
        profile_id.as_deref(),
        directory,
        allowed_tools,
    )
    .await?;
    Ok(tree)
}

/// Write the session record naming the chat executor, so the session appears in
/// listings and `require_chat_session` accepts it.
pub async fn record_session(
    ctx: &ApiContext,
    session_id: &str,
    profile_id: Option<&str>,
    directory: Option<String>,
    allowed_tools: Option<Vec<String>>,
) -> ApiResult<()> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_types::agent_execution::AgentRuntimeConfig;
    use wf_types::{AgentExecution, ExecutionStatus, Id};

    let existing = ctx
        .storage
        .agent_execution
        .load(session_id)
        .await
        .ok()
        .flatten();
    // A later turn may omit the profile (the tree keeps it), so the record
    // keeps the profile it already had instead of losing it.
    let profile_id = profile_id.map(str::to_string).or_else(|| {
        existing
            .as_ref()
            .and_then(|record| record.context.as_ref())
            .and_then(|context| context.profile_id.clone())
    });
    // The allow-list is session-scoped: a turn call passes `None` here and must
    // not erase a list configured at creation. Only an explicit Some replaces it.
    let mut base_context = existing
        .as_ref()
        .and_then(|record| record.context.clone())
        .unwrap_or_default();
    if let Some(tools) = allowed_tools {
        base_context.available_tools = Some(tools);
    }
    let record = AgentExecution {
        id: Id::from(session_id.to_string()),
        definition_id: Id::from(crate::chat::CHAT_SESSION_DEFINITION_ID.to_string()),
        execution_type: wf_types::execution::ExecutionType::Chat,
        status: existing
            .as_ref()
            .map(|record| record.status.clone())
            .unwrap_or(ExecutionStatus::Running),
        current_iteration: 0,
        tool_call_count: 0,
        iteration_history: None,
        started_at: existing
            .as_ref()
            .map(|record| record.started_at)
            .unwrap_or_else(wf_common::now),
        completed_at: existing.as_ref().and_then(|record| record.completed_at),
        error: existing.as_ref().and_then(|record| record.error.clone()),
        context: Some(AgentRuntimeConfig {
            profile_id,
            ..base_context
        }),
        loop_config: None,
        permanently_failed_tools: None,
        hierarchy: None,
        rerun_of: None,
        directory: existing
            .as_ref()
            .and_then(|record| record.directory.clone())
            .or(directory),
    };
    ctx.storage
        .agent_execution
        .save(&record)
        .await
        .map_err(ApiError::from)
}

/// Update only the status of the session record, leaving its profile and
/// directory as they are.
pub async fn touch_session_status(
    ctx: &ApiContext,
    session_id: &str,
    status: wf_types::ExecutionStatus,
    error: Option<String>,
) {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let Ok(Some(mut record)) = ctx.storage.agent_execution.load(session_id).await else {
        return;
    };
    record.status = status;
    record.error = error;
    record.completed_at = Some(wf_common::now());
    if let Err(error) = ctx.storage.agent_execution.save(&record).await {
        tracing::warn!(
            target: "wf_api::conversation",
            session_id,
            error = %error,
            "failed to update chat session status"
        );
    }
}

/// Digest index of a loaded tree.
///
/// The index is pure computation over the document, so it is rebuilt on every
/// load instead of persisted: a stored digest could disagree with the document
/// it claims to summarise, and every staleness decision would inherit the lie.
pub fn digests(tree: &ConversationTree) -> DigestIndex {
    conversation_tree::build(tree)
}

/// Remove a consumed graft from its source session's pool.
///
/// Cross-session grafting moves the document into the target tree, so the
/// source tree must drop its index entry without touching the moved nodes.
pub async fn remove_pool_entry(
    ctx: &ApiContext,
    session_id: &str,
    graft_id: &str,
) -> ApiResult<()> {
    let runtime = super::runtime::runtime_for(session_id);
    let _guard = runtime.write().await;
    let mut tree = match load_existing(ctx, session_id).await? {
        Some(tree) => tree,
        None => return Ok(()),
    };
    let before = tree.pool.len();
    tree.pool.retain(|entry| entry.graft_id != graft_id);
    if tree.pool.len() == before {
        return Ok(());
    }
    tree.version += 1;
    tree.updated_at = wf_common::now();
    save_tree(ctx, &tree).await
}

/// Delete the session's tree documents: the session tree and every pooled
/// graft.
pub async fn delete_documents(ctx: &ApiContext, session_id: &str) -> ApiResult<usize> {
    let trees = ctx
        .storage
        .conversation_tree
        .list_by_session(session_id)
        .await
        .map_err(ApiError::from)?;
    let mut removed = 0usize;
    for tree in trees {
        if ctx
            .storage
            .conversation_tree
            .delete(&tree.id)
            .await
            .map_err(ApiError::from)?
        {
            removed += 1;
        }
    }
    Ok(removed)
}
