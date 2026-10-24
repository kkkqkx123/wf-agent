//! Draft lifecycle shared by every editable definition kind.
//!
//! A draft is an unpublished definition under edit. Every kind answers the
//! same questions about a draft — does one exist, which lifecycle state is
//! it in, can it be promoted, what does promoting it produce — and this
//! module owns those answers. What stays with the definition it belongs to
//! is the payload, the parse-level check and the formal save: those differ
//! per kind and sit next to the definition they describe.
//!
//! The service is parameterized by the kind of definition rather than by
//! the definition itself, so a caller that knows "this is an agent draft"
//! reaches the same lifecycle rules as one that knows "this is a workflow
//! draft" without either side re-implementing them.

use serde::Serialize;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::ValidationError;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// The kinds of definition that can be edited and therefore can carry a
/// draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    Workflow,
    Agent,
    Node,
    Trigger,
}

impl DefinitionKind {
    /// Stable wire name of the definition kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workflow => "workflow",
            Self::Agent => "agent",
            Self::Node => "node",
            Self::Trigger => "trigger",
        }
    }
}

/// Lifecycle view of a definition id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Draft,
    Formal,
    Expired,
}

impl LifecycleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecycleStatus::Draft => "draft",
            LifecycleStatus::Formal => "formal",
            LifecycleStatus::Expired => "expired",
        }
    }
}

/// What a promotion produced.
#[derive(Debug, Clone)]
pub struct PromotionOutcome {
    /// Issues the formal save raised without rejecting it.
    pub warnings: Vec<ValidationError>,
    /// The definitions that reference the promoted one, with the level each
    /// reached after revalidation.
    pub impact: crate::infra::dependency::UpdateImpactReport,
}

/// Lifecycle view of a definition id.
///
/// A draft is an editable document; the formal definition is the one that
/// runs. A workflow that exists but is stale is marked expired, because the
/// stored graph no longer matches what its dependants reference.
pub async fn lifecycle_of(
    ctx: &ApiContext,
    kind: DefinitionKind,
    id: &str,
) -> Option<LifecycleStatus> {
    if draft_exists(ctx, kind, id).await {
        return Some(LifecycleStatus::Draft);
    }
    if is_stale(ctx, kind, id) {
        return Some(LifecycleStatus::Expired);
    }
    Some(LifecycleStatus::Formal)
}

/// Whether a draft of this kind exists for the id.
pub async fn draft_exists(ctx: &ApiContext, kind: DefinitionKind, id: &str) -> bool {
    match kind {
        DefinitionKind::Workflow => ctx.storage.workflow_draft.exists(id).await.unwrap_or(false),
        DefinitionKind::Agent => ctx.storage.agent_draft.exists(id).await.unwrap_or(false),
        DefinitionKind::Node => ctx.storage.node_draft.exists(id).await.unwrap_or(false),
        DefinitionKind::Trigger => ctx.storage.trigger_draft.exists(id).await.unwrap_or(false),
    }
}

/// Delete a draft. Reports whether one was there.
pub async fn delete_draft(ctx: &ApiContext, kind: DefinitionKind, id: &str) -> ApiResult<bool> {
    match kind {
        DefinitionKind::Workflow => Ok(ctx.storage.workflow_draft.delete(id).await?),
        DefinitionKind::Agent => Ok(ctx.storage.agent_draft.delete(id).await?),
        DefinitionKind::Node => Ok(ctx.storage.node_draft.delete(id).await?),
        DefinitionKind::Trigger => Ok(ctx.storage.trigger_draft.delete(id).await?),
    }
}

/// Complete validation preview for promotion.
///
/// The returned issues are the ones that do not block the promotion; a draft
/// that cannot be promoted is rejected with the reason instead.
pub async fn validate_draft_complete(
    ctx: &ApiContext,
    kind: DefinitionKind,
    id: &str,
) -> ApiResult<Vec<ValidationError>> {
    match kind {
        DefinitionKind::Workflow => {
            let draft = crate::workflow::draft::get_draft(ctx, id).await?;
            crate::workflow::validation::validate_workflow_for_publish(ctx, &draft)
                .await
                .map_err(|e| promotion_error(kind, id, e))
        }
        DefinitionKind::Agent => {
            let draft = crate::agent::agent_draft::get_draft(ctx, id).await?;
            crate::agent::validate_agent_definition(ctx, &draft)
                .map_err(|e| promotion_error(kind, id, e))
        }
        DefinitionKind::Node => {
            let draft = crate::template::node_draft::get_draft(ctx, id).await?;
            let result = crate::template::node_template::validate_node_template_dry_run(&draft);
            complete_dry_run(kind, id, result).map_err(|e| promotion_error(kind, id, e))
        }
        DefinitionKind::Trigger => {
            let draft = crate::trigger::trigger_draft::get_draft(ctx, id).await?;
            let result = crate::trigger::template::validate_dry_run(ctx, &draft).await?;
            complete_dry_run(kind, id, result).map_err(|e| promotion_error(kind, id, e))
        }
    }
}

/// Turn a dry-run validation result into promotion preview output.
///
/// An invalid draft cannot be promoted and is rejected with the reason;
/// a valid one promotes with its warnings.
fn complete_dry_run(
    kind: DefinitionKind,
    id: &str,
    result: wf_types::ValidationResult,
) -> ApiResult<Vec<ValidationError>> {
    if result.is_valid() {
        return Ok(result.warnings);
    }
    let detail = result
        .errors
        .iter()
        .map(|e| format!("{}: {}", e.field, e.message))
        .collect::<Vec<_>>()
        .join("; ");
    Err(ApiError::Validation(format!(
        "draft {} '{id}' cannot be promoted: {detail}",
        kind.as_str()
    )))
}

/// Promote a draft to its formal definition.
///
/// The formal version in place is snapshotted first, so a promotion can be
/// undone by rolling back to it. The draft is removed once the formal
/// definition is in place.
pub async fn promote_draft(
    ctx: &ApiContext,
    kind: DefinitionKind,
    id: &str,
) -> ApiResult<PromotionOutcome> {
    match kind {
        DefinitionKind::Workflow => {
            let mut draft = crate::workflow::draft::get_draft(ctx, id).await?;
            // A canvas draft is a revision of the workflow it was derived
            // from, so promoting it produces that workflow's formal
            // definition rather than a second one.
            let owner = crate::workflow::draft::draft_owner_id(ctx, id).await?;
            draft.id = owner.clone();
            snapshot_workflow_version(ctx, &owner).await;
            let impact =
                crate::workflow::definition::save_workflow_with_impact(ctx, &draft).await?;
            let _ = delete_draft(ctx, kind, id).await;
            Ok(PromotionOutcome {
                warnings: Vec::new(),
                impact,
            })
        }
        DefinitionKind::Agent => {
            let draft = crate::agent::agent_draft::get_draft(ctx, id).await?;
            snapshot_agent_version(ctx, id).await;
            let warnings = crate::agent::save_agent_template(ctx, &draft).await?;
            let impact = crate::infra::dependency::check_update_impact(
                ctx,
                crate::infra::dependency::DependencyKind::SubWorkflow,
                id,
            )
            .await?;
            let _ = delete_draft(ctx, kind, id).await;
            Ok(PromotionOutcome { warnings, impact })
        }
        DefinitionKind::Node => {
            let draft = crate::template::node_draft::get_draft(ctx, id).await?;
            snapshot_node_version(ctx, id).await;
            crate::template::node_template::save_node_template(ctx, &draft).await?;
            let impact = crate::infra::dependency::check_update_impact(
                ctx,
                crate::infra::dependency::DependencyKind::Node,
                id,
            )
            .await?;
            let _ = delete_draft(ctx, kind, id).await;
            Ok(PromotionOutcome {
                warnings: Vec::new(),
                impact,
            })
        }
        DefinitionKind::Trigger => {
            let draft = crate::trigger::trigger_draft::get_draft(ctx, id).await?;
            snapshot_trigger_version(ctx, id).await;
            crate::trigger::template::save(ctx, &draft).await?;
            let impact = crate::infra::dependency::check_update_impact(
                ctx,
                crate::infra::dependency::DependencyKind::Trigger,
                id,
            )
            .await?;
            let _ = delete_draft(ctx, kind, id).await;
            Ok(PromotionOutcome {
                warnings: Vec::new(),
                impact,
            })
        }
    }
}

/// The rejection a promotion fails with: the reason, named by the kind of
/// definition that produced it.
fn promotion_error(kind: DefinitionKind, id: &str, error: ApiError) -> ApiError {
    match error {
        ApiError::Validation(detail) => ApiError::Validation(format!(
            "draft {} '{id}' cannot be promoted: {detail}",
            kind.as_str()
        )),
        other => other,
    }
}

/// Whether the formal definition of this id is stale.
fn is_stale(ctx: &ApiContext, kind: DefinitionKind, id: &str) -> bool {
    match kind {
        DefinitionKind::Workflow => ctx.is_stale(id),
        // Only workflows carry a graph whose shape can drift from its
        // dependants; every other kind never goes stale on its own.
        DefinitionKind::Agent | DefinitionKind::Node | DefinitionKind::Trigger => false,
    }
}

/// Snapshot the formal workflow in place, so the promotion can be undone.
///
/// A workflow that has never been saved has no version to snapshot; that is
/// not an error, it only means there is nothing to roll back to.
async fn snapshot_workflow_version(ctx: &ApiContext, id: &str) {
    let Ok(current) = crate::workflow::definition::get_workflow(ctx, id).await else {
        return;
    };
    let label = current
        .version
        .clone()
        .unwrap_or_else(|| format!("pre-promote-{}", wf_common::now()));
    let _ = crate::workflow::version::save_workflow_version(ctx, id, &label, &current).await;
}

/// Snapshot the formal agent template in place, so the promotion can be
/// undone.
async fn snapshot_agent_version(ctx: &ApiContext, id: &str) {
    let Ok(current) = crate::template::template_library::get_agent_template(ctx, id) else {
        return;
    };
    let label = current
        .definition
        .version
        .clone()
        .unwrap_or_else(|| format!("pre-promote-{}", wf_common::now()));
    let _ =
        crate::template::template_library::save_agent_template_version(ctx, id, &label, &current)
            .await;
}

/// Snapshot the formal node template in place, so the promotion can be
/// undone.
async fn snapshot_node_version(ctx: &ApiContext, id: &str) {
    let Ok(current) = crate::template::node_template::get_node_template(&ctx.storage, id).await
    else {
        return;
    };
    let label = format!("pre-promote-{}", wf_common::now());
    let _ =
        crate::template::node_version::save_node_template_version(ctx, id, &label, &current).await;
}

/// Snapshot the formal trigger template in place, so the promotion can be
/// undone.
async fn snapshot_trigger_version(ctx: &ApiContext, id: &str) {
    let Ok(current) = crate::trigger::template::get(ctx, id).await else {
        return;
    };
    let label = format!("pre-promote-{}", wf_common::now());
    let _ = crate::trigger::version::save_trigger_template_version(ctx, id, &label, &current).await;
}
