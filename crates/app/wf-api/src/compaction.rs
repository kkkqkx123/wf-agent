//! Manual compaction face for live agent loops.
//!
//! A `/compact` request resolves the live execution handle, drives the
//! engine bound to that instance through `AgentLoopEntity::compact_now_applied`,
//! and returns a stable, serializable summary. The engine owns the decision;
//! the host owns the write-back. No execution state is mutated here beyond
//! the compaction itself, and a missing/terminal loop is a not-found error.

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// One manual compaction request.
#[derive(Debug, Clone)]
pub struct CompactParams {
    /// Live agent loop id to compact.
    pub loop_id: String,
    /// Caller-supplied reason recorded in the manual request.
    pub reason: String,
}

/// Serializable result of one manual compaction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CompactResult {
    /// Whether a write-back landed. `Noop`/`View` outcomes report `false`
    /// here because their effect is routed through the summary-signal path.
    pub applied: bool,
    /// Outcome kind: `noop`, `view`, `replace` or `error`.
    pub kind: String,
    /// Bound engine name, when the loop carries one.
    pub engine: Option<String>,
    /// Human-readable detail (rejection reason on error).
    pub detail: String,
}

/// Trigger one manual compaction on the live agent loop identified by
/// `params.loop_id`. Unknown ids report `ExecutionNotFound`; a loop without
/// a bound engine reports a validation error (compaction disabled).
pub async fn compact_now(ctx: &ApiContext, params: CompactParams) -> ApiResult<CompactResult> {
    let loop_id = params.loop_id.trim();
    if loop_id.is_empty() {
        return Err(ApiError::Validation(
            "compact requires a loop id".to_string(),
        ));
    }
    let entity = ctx
        .agent_loop(loop_id)
        .ok_or_else(|| ApiError::ExecutionNotFound {
            id: loop_id.to_string(),
        })?;
    let engine = entity
        .compaction_engine_snapshot()
        .0
        .unwrap_or_else(|| "builtin".to_string());
    let reason = if params.reason.trim().is_empty() {
        "manual".to_string()
    } else {
        params.reason
    };
    let outcome = entity.compact_now_applied(&reason).await.ok_or_else(|| {
        ApiError::Validation("no compaction engine is bound to this execution".to_string())
    })?;
    match outcome {
        Ok((outcome, proof)) => {
            let (kind, applied, detail) = match outcome {
                wf_execution_shared::CompactionOutcome::Noop => {
                    ("noop", false, "engine returned no compaction".to_string())
                }
                wf_execution_shared::CompactionOutcome::View(plan) => (
                    "view",
                    false,
                    format!(
                        "view plan emitted ({} items, {} freed tokens)",
                        plan.items.len(),
                        plan.freed_tokens
                    ),
                ),
                wf_execution_shared::CompactionOutcome::Replace(_) => {
                    let reclaim = proof
                        .map(|p| p.old_tokens.saturating_sub(p.new_tokens))
                        .unwrap_or(0);
                    (
                        "replace",
                        true,
                        format!("destructive replacement applied; reclaimed {reclaim} tokens"),
                    )
                }
            };
            Ok(CompactResult {
                applied,
                kind: kind.to_string(),
                engine: Some(engine),
                detail,
            })
        }
        Err(e) => Ok(CompactResult {
            applied: false,
            kind: "error".to_string(),
            engine: Some(engine),
            detail: e.to_string(),
        }),
    }
}
