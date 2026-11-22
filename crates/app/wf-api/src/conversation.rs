//! Conversation branch tree over the tool-free chat primitive.
//!
//! A chat session used to be an append-only message list: one question, one
//! answer, no edges. This module gives the same session a tree, so a
//! conversation can be re-aimed after the fact — add/replace/remove a
//! predecessor, rewrite either side, graft a subtree elsewhere — and keeps the
//! answers that no longer match their history visibly marked instead of
//! silently sending them to the model.
//!
//! Layout:
//!
//! * [`store`] — loading a session's tree, adopting a legacy linear session.
//! * [`runtime`] — per-session write lock and the single in-flight turn slot.
//! * [`events`] — the `BaseEvent`s the driver publishes for SSE subscribers.
//! * [`turn`] — submitting a turn and driving it to convergence.
//! * [`step`] — regenerating stale answers, one at a time or in order.
//! * [`surgery`] — the structural and content operations.
//! * [`query`] — snapshots, path projections and the dirty list.
//!
//! Rules that hold everywhere below:
//!
//! * A message is immutable once stored; changing what a node says writes a new
//!   message and repoints the node.
//! * Staleness is derived from content digests, never stored, so undoing an
//!   edit restores freshness without cleanup.
//! * Only the path matters for context, so "switching branch" is a leaf move.

pub mod events;
pub mod query;
pub mod runtime;
pub mod step;
pub mod store;
pub mod stream;
pub mod surgery;
pub mod turn;

#[cfg(test)]
mod tests;

pub use query::{dirty_list, path_messages, snapshot, PathEntry, SessionSnapshot};
pub use step::{start_step, StepHandle, StepMode, StepRequest};
pub use store::{create_session, load_or_adopt, require_tree, save_tree};
pub use stream::subscribe;
pub use surgery::{
    acknowledge_reply, attach_graft, delete_node, delete_session, detach_subtree, discard_reply,
    edit_branch, insert_before, list_grafts, reparent, restore_reply, set_active_leaf, splice_out,
    write_reply_body, write_user_content, InsertRequest, NodeChange, SurgeryRequest,
};
pub use turn::{cancel_turn, submit_turn, TurnHandle, TurnRequest};

use wf_execution_shared::conversation_tree::TreeError;

use crate::infra::error::ApiError;

/// Translate a tree rule violation into the transport error the endpoints
/// return.
///
/// A missing anchor is a 404, a rejected shape (cycle, depth, node cap) is a
/// 400 — the caller asked for something the tree cannot be — and a duplicated
/// ordinal means the document is inconsistent, which is a conflict the caller
/// must not blindly retry.
pub(crate) fn tree_error(error: TreeError) -> ApiError {
    match error {
        TreeError::MissingNode(id) => ApiError::not_found("conversation node", &id),
        TreeError::MissingParent { id, parent } => {
            ApiError::Validation(format!("node '{id}' points at missing parent '{parent}'"))
        }
        TreeError::Cycle(id) => {
            ApiError::Validation(format!("move would create a cycle through '{id}'"))
        }
        TreeError::Unreachable(id) => {
            ApiError::Validation(format!("node '{id}' is not reachable from the root"))
        }
        TreeError::DuplicateOrdinal { parent, ordinal } => ApiError::Conflict(format!(
            "sibling ordinal {ordinal} is duplicated under '{parent}'"
        )),
        TreeError::BadActiveLeaf(id) => {
            ApiError::Validation(format!("active leaf '{id}' does not exist"))
        }
        TreeError::PremiseWithReply(id) => {
            ApiError::Validation(format!("premise '{id}' cannot carry a reply"))
        }
        TreeError::TooManyNodes { count, limit } => ApiError::Validation(format!(
            "tree would hold {count} nodes, over the limit of {limit}"
        )),
        TreeError::TooDeep { depth, limit } => {
            ApiError::Validation(format!("tree depth {depth} exceeds the limit of {limit}"))
        }
    }
}

/// Reject a branch that only exists for another execution kind.
///
/// `ExecutionType::Chat` is the only session type with a tree; an agent loop or
/// a workflow execution that happens to share the id space must not be turned
/// into one by a chat request.
pub(crate) async fn require_chat_session(
    ctx: &crate::infra::context::ApiContext,
    session_id: &str,
) -> crate::infra::error::ApiResult<()> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_types::execution::ExecutionType;

    let Some(record) = ctx.storage.agent_execution.load(session_id).await? else {
        return Ok(());
    };
    if record.execution_type == ExecutionType::Chat {
        return Ok(());
    }
    Err(ApiError::Validation(format!(
        "session '{session_id}' is a {} execution, not a chat session",
        record.execution_type.as_str()
    )))
}
