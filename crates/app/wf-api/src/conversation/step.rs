//! Regenerating stale answers: one at a time, or in order down a path.
//!
//! Editing a premise invalidates every answer below it. Regenerating them all
//! at once hides which answer changed what, so the tree offers two shapes of the
//! same operation:
//!
//! * **single** — regenerate one answer, look at it, decide;
//! * **forward** — walk the stale answers of one path from the top.
//!
//! The plan is snapshotted when the request arrives, so the run cannot chase its
//! own tail, but each answer is re-checked before it is generated: if an upstream
//! regeneration happened to reproduce the same body, the downstream digest does
//! not move and that answer must not be regenerated for nothing.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use wf_execution_shared::conversation_tree::{self, TreeEditor};
use wf_types::{ParamsFingerprint, ReplyOrigin, ReplyState};

use super::query::session_profile;
use super::runtime::{self, SessionRuntime};
use super::{events, store, turn};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Default ceiling of one forward run. A single edit typically invalidates two
/// or three answers; eight leaves room without letting a runaway plan spend a
/// whole context window's worth of generations.
pub const DEFAULT_MAX_STEPS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Regenerate the first stale answer only.
    Single,
    /// Walk the stale answers of the path from the top.
    Forward,
}

impl StepMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Forward => "forward",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StepRequest {
    pub session_id: String,
    /// Path to step. Absent steps the active leaf's path.
    pub leaf: Option<String>,
    pub mode: StepMode,
    /// Stop before regenerating this node (inclusive stop, nothing generated).
    pub stop_at: Option<String>,
    pub max_steps: usize,
    pub profile_id: Option<String>,
    pub if_version: Option<u64>,
    /// Regenerate this node instead of the first stale node on the path.
    pub node_id: Option<String>,
    /// `keep_human` refuses to overwrite a human-written answer.
    pub policy: Option<String>,
}

impl Default for StepMode {
    fn default() -> Self {
        Self::Single
    }
}

/// Handle of a stepping run: the planned nodes, in order.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StepHandle {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf: Option<String>,
    pub plan: Vec<String>,
}

/// Plan and start a stepping run.
///
/// A run that has nothing to do returns an empty plan and starts no job: the
/// caller sees "nothing stale" instead of a job that finishes immediately.
pub async fn start_step(ctx: Arc<ApiContext>, request: StepRequest) -> ApiResult<StepHandle> {
    super::require_chat_session(&ctx, &request.session_id).await?;
    let runtime = runtime::runtime_for(&request.session_id);
    let job_id = wf_common::generate_id();
    let cancel = CancellationToken::new();
    let keep_human = request.policy.as_deref() != Some("overwrite_human");
    let (plan, leaf, profile_id) = {
        let _guard = runtime.write().await;
        if runtime.inflight().is_some() {
            return Err(ApiError::Conflict(
                "session is generating a turn; wait for it to settle".into(),
            ));
        }
        let tree = store::load_or_adopt(&ctx, &request.session_id).await?;
        if let Some(expected) = request.if_version {
            if tree.version != expected {
                return Err(ApiError::Conflict(format!(
                    "tree is at version {}, not {expected}",
                    tree.version
                )));
            }
        }
        let digests = store::digests(&tree);
        let mut plan = conversation_tree::plan_steps(&tree, &digests, request.leaf.as_deref());
        if let Some(node_id) = request.node_id.as_deref() {
            if !plan.nodes.iter().any(|candidate| candidate == node_id) {
                let exists = tree.node(node_id).is_some();
                plan.nodes.clear();
                if !exists {
                    return Err(ApiError::not_found("conversation node", node_id));
                }
            } else {
                plan.nodes.retain(|candidate| candidate == node_id);
            }
        }
        if keep_human {
            for node_id in &plan.nodes {
                if tree
                    .node(node_id)
                    .and_then(|node| node.reply.as_ref())
                    .is_some_and(|reply| reply.origin == ReplyOrigin::Human)
                {
                    return Err(ApiError::Conflict(format!(
                        "node '{node_id}' was written by a human; set policy to overwrite_human to regenerate it"
                    )));
                }
            }
        }
        let leaf = request.leaf.clone().or_else(|| tree.active_leaf.clone());
        let profile_id = match request
            .profile_id
            .clone()
            .or_else(|| tree.profile_id.clone())
        {
            Some(profile) if !profile.trim().is_empty() => Some(profile),
            _ => session_profile(&ctx, &tree).await,
        };
        (plan, leaf, profile_id)
    };

    if plan.is_empty() {
        return Ok(StepHandle {
            job_id: String::new(),
            leaf,
            plan: Vec::new(),
        });
    }
    let profile_id = profile_id
        .filter(|profile| !profile.trim().is_empty())
        .ok_or_else(|| ApiError::Validation("stepping requires a profile id".into()))?;

    let mut nodes = plan.nodes.clone();
    if request.mode == StepMode::Single {
        nodes.truncate(1);
    }
    runtime.begin_job(&job_id, &nodes[0], cancel.clone())?;
    events::step_started(
        &ctx,
        &request.session_id,
        &job_id,
        request.mode.as_str(),
        &nodes,
    );

    let job = StepJob {
        ctx: ctx.clone(),
        session_id: request.session_id.clone(),
        job_id: job_id.clone(),
        nodes: nodes.clone(),
        stop_at: request.stop_at.clone(),
        max_steps: if request.max_steps == 0 {
            DEFAULT_MAX_STEPS
        } else {
            request.max_steps
        },
        profile_id,
        keep_human,
        cancel,
        runtime,
    };
    let task = tokio::spawn(async move { job.run().await });
    ctx.execution_tasks
        .register(job_id.clone(), task.abort_handle());
    Ok(StepHandle {
        job_id,
        leaf,
        plan: nodes,
    })
}

struct StepJob {
    ctx: Arc<ApiContext>,
    session_id: String,
    job_id: String,
    nodes: Vec<String>,
    stop_at: Option<String>,
    max_steps: usize,
    profile_id: String,
    keep_human: bool,
    cancel: CancellationToken,
    runtime: Arc<SessionRuntime>,
}

impl StepJob {
    async fn run(self) {
        let total = self.nodes.len();
        let mut done = 0usize;
        let mut reason = "completed";
        let mut last: Option<String> = None;

        for node_id in self.nodes.clone() {
            if self.stop_at.as_deref() == Some(node_id.as_str()) {
                reason = "stop_at";
                break;
            }
            if done >= self.max_steps {
                reason = "max_steps";
                break;
            }
            if self.cancel.is_cancelled() {
                reason = "cancelled";
                break;
            }
            let turn_id = wf_common::generate_id();
            let Some(parent) = self.regenerate(&node_id, &turn_id).await else {
                if self.cancel.is_cancelled() {
                    reason = "cancelled";
                    break;
                }
                continue;
            };
            events::turn_started(
                &self.ctx,
                &self.session_id,
                &turn_id,
                &node_id,
                &parent,
            );
            let answer = turn::generate_answer(
                &self.ctx,
                &self.session_id,
                &turn_id,
                &node_id,
                &self.profile_id,
                &self.cancel,
                &self.runtime,
            )
            .await;
            let state = answer.state;
            turn::converge_answer(
                &self.ctx,
                &self.session_id,
                &turn_id,
                &node_id,
                answer,
                &self.runtime,
            )
            .await;
            done += 1;
            last = Some(node_id.clone());
            events::step_progress(
                &self.ctx,
                &self.session_id,
                &self.job_id,
                done,
                total,
                &node_id,
            );
            if state == ReplyState::Failed {
                reason = "failed";
                break;
            }
        }

        events::step_finished(
            &self.ctx,
            &self.session_id,
            &self.job_id,
            reason,
            done,
            last.as_deref(),
        );
        self.runtime.finish_job(&self.job_id);
        self.ctx.execution_tasks.unregister(&self.job_id);
    }

    /// Put one node into the pending state inside the job's existing slot.
    ///
    /// Returns the parent id when generation should proceed, `None` when the
    /// plan is already stale by the time this step runs.
    async fn regenerate(&self, node_id: &str, turn_id: &str) -> Option<String> {
        let runtime = self.runtime.clone();
        let _guard = runtime.write().await;
        let Ok(Some(tree)) = store::load_existing(&self.ctx, &self.session_id).await else {
            return None;
        };
        let digests = store::digests(&tree);
        let Some(node) = tree.node(node_id) else {
            return None;
        };
        if node.is_deleted() || !conversation_tree::still_stale(&tree, &digests, node_id) {
            return None;
        }
        if self.keep_human
            && node
                .reply
                .as_ref()
                .is_some_and(|reply| reply.origin == ReplyOrigin::Human)
        {
            return None;
        }
        let parent = node.parent.clone();
        if self
            .runtime
            .advance_job(&self.job_id, turn_id, node_id)
            .is_err()
        {
            return None;
        }
        let needs_placeholder = node.reply.is_none();
        let mut tree = tree;
        let now = wf_common::now();
        let params = Some(ParamsFingerprint {
            profile_id: self.profile_id.clone(),
            parameters: None,
            tools: Vec::new(),
        });
        let mut digests = digests;
        if !needs_placeholder {
            let mut editor = TreeEditor::new(&mut tree, &mut digests, now);
            // A regeneration archives the previous body and puts a pending
            // placeholder in place, so the tree never shows stale text as if it
            // were current.
            if let Err(error) = editor.begin_regeneration(node_id, params) {
                tracing::warn!(
                    target: "wf_api::conversation",
                    node_id,
                    error = %error,
                    "failed to begin regeneration"
                );
                return None;
            }
        }
        if let Err(error) = store::save_tree(&self.ctx, &tree).await {
            tracing::warn!(
                target: "wf_api::conversation",
                node_id,
                error = %error,
                "failed to persist the pending regeneration"
            );
            return None;
        }
        Some(parent)
    }
}
