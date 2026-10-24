//! Draft versus formal trigger template lifecycle.
//!
//! A trigger template draft is an editable template that promotes into the
//! formal trigger template store. The lifecycle itself is shared with every
//! other editable definition kind and lives in
//! `crate::definition::draft`; this module holds what is specific to a
//! trigger template: its payload and its parse-level check.

use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Parse-level draft check: only id and name are required. Full shape and
/// reference validation runs at promotion, so half-finished edits can be
/// stored.
pub fn validate_draft_parse(template: &wf_types::TriggerTemplateStorageMetadata) -> ApiResult<()> {
    if template.id.as_str().trim().is_empty() {
        return Err(ApiError::Validation(
            "draft trigger template id must not be empty".into(),
        ));
    }
    if template.name.trim().is_empty() {
        return Err(ApiError::Validation(
            "draft trigger template name must not be empty".into(),
        ));
    }
    Ok(())
}

pub async fn save_draft(
    ctx: &ApiContext,
    template: &wf_types::TriggerTemplateStorageMetadata,
) -> ApiResult<()> {
    validate_draft_parse(template)?;
    ctx.storage.trigger_draft.save(template).await?;
    Ok(())
}

pub async fn get_draft(
    ctx: &ApiContext,
    id: &str,
) -> ApiResult<wf_types::TriggerTemplateStorageMetadata> {
    ctx.storage
        .trigger_draft
        .load(id)
        .await?
        .ok_or_else(|| ApiError::not_found("trigger_draft", id))
}

pub async fn list_drafts(
    ctx: &ApiContext,
) -> ApiResult<Vec<wf_types::TriggerTemplateStorageMetadata>> {
    ctx.storage
        .trigger_draft
        .list(None)
        .await
        .map_err(Into::into)
}

/// The drafts that belong to one trigger template.
///
/// A draft is a revision of one definition, so it is stored under that
/// definition's id; the drafts of a definition are the ones filed under
/// it.
pub async fn list_drafts_of(
    ctx: &ApiContext,
    definition_id: &str,
) -> ApiResult<Vec<wf_types::TriggerTemplateStorageMetadata>> {
    Ok(list_drafts(ctx)
        .await?
        .into_iter()
        .filter(|draft| draft.id.as_str() == definition_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    fn draft_template(id: &str) -> wf_types::TriggerTemplateStorageMetadata {
        wf_types::TriggerTemplateStorageMetadata {
            id: id.into(),
            name: format!("draft-{id}"),
            trigger_type: "event".into(),
            description: None,
            category: None,
            tags: None,
            enabled: true,
            max_triggers: None,
            priority: None,
            dispatch_mode: None,
            condition: None,
            action_config: None,
            derived_from: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn draft_trigger_template_promotes_to_formal() {
        let ctx = make_ctx();
        save_draft(&ctx, &draft_template("trigger-draft"))
            .await
            .unwrap();
        assert_eq!(
            crate::definition::draft::lifecycle_of(
                &ctx,
                crate::definition::draft::DefinitionKind::Trigger,
                "trigger-draft"
            )
            .await,
            Some(crate::definition::draft::LifecycleStatus::Draft)
        );
        crate::definition::draft::promote_draft(
            &ctx,
            crate::definition::draft::DefinitionKind::Trigger,
            "trigger-draft",
        )
        .await
        .unwrap();
        assert!(get_draft(&ctx, "trigger-draft").await.is_err());
        assert!(crate::trigger::template::get(&ctx, "trigger-draft")
            .await
            .is_ok());
    }
}
