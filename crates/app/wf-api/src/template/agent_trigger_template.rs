//! Trigger template registry.

use std::sync::Arc;

use serde::Serialize;

use wf_core::registry::MutableRegistry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::trigger_template::TriggerTemplateListOptions;
use wf_types::trigger::TriggerTemplate;
use wf_types::TriggerTemplateStorageMetadata;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiResult};

/// Trigger template filter.
#[derive(Debug, Clone, Default)]
pub struct AgentTriggerTemplateFilter {
    pub trigger_type: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub name: Option<String>,
}

/// Digest of a trigger template.
#[derive(Debug, Clone, Serialize)]
pub struct AgentTriggerTemplateSummary {
    pub id: String,
    pub name: String,
    pub trigger_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub enabled: bool,
    pub updated_at: i64,
}

/// Query trigger templates with an optional filter.
pub async fn query(
    ctx: &ApiContext,
    filter: Option<&AgentTriggerTemplateFilter>,
) -> ApiResult<Vec<TriggerTemplateStorageMetadata>> {
    let options = filter.map(|f| TriggerTemplateListOptions {
        offset: None,
        limit: None,
        trigger_type_filter: f.trigger_type.clone(),
        name_filter: f.name.clone(),
        category_filter: f.category.clone(),
        enabled_filter: f.enabled,
    });
    let mut templates = ctx.storage.trigger_template.list(options).await?;
    if let Some(filter) = filter {
        if let Some(tags) = &filter.tags {
            templates.retain(|t| {
                tags.is_empty()
                    || t.tags
                        .as_ref()
                        .map(|existing| tags.iter().any(|tag| existing.contains(tag)))
                        .unwrap_or(false)
            });
        }
    }
    Ok(templates)
}

pub async fn query_by_type(
    ctx: &ApiContext,
    trigger_type: &str,
) -> ApiResult<Vec<TriggerTemplateStorageMetadata>> {
    query(
        ctx,
        Some(&AgentTriggerTemplateFilter {
            trigger_type: Some(trigger_type.to_string()),
            ..AgentTriggerTemplateFilter::default()
        }),
    )
    .await
}

pub async fn query_by_category(
    ctx: &ApiContext,
    category: &str,
) -> ApiResult<Vec<TriggerTemplateStorageMetadata>> {
    query(
        ctx,
        Some(&AgentTriggerTemplateFilter {
            category: Some(category.to_string()),
            ..AgentTriggerTemplateFilter::default()
        }),
    )
    .await
}

pub async fn query_by_tags(
    ctx: &ApiContext,
    tags: &[String],
) -> ApiResult<Vec<TriggerTemplateStorageMetadata>> {
    query(
        ctx,
        Some(&AgentTriggerTemplateFilter {
            tags: Some(tags.to_vec()),
            ..AgentTriggerTemplateFilter::default()
        }),
    )
    .await
}

/// Template summaries, optionally filtered.
pub async fn summaries(
    ctx: &ApiContext,
    filter: Option<&AgentTriggerTemplateFilter>,
) -> ApiResult<Vec<AgentTriggerTemplateSummary>> {
    Ok(query(ctx, filter)
        .await?
        .into_iter()
        .map(|t| AgentTriggerTemplateSummary {
            id: t.id.to_string(),
            name: t.name,
            trigger_type: t.trigger_type,
            description: t.description,
            category: t.category,
            tags: t.tags,
            enabled: t.enabled,
            updated_at: t.updated_at,
        })
        .collect())
}

/// Keyword search over template names / descriptions.
pub async fn search(
    ctx: &ApiContext,
    keyword: &str,
) -> ApiResult<Vec<TriggerTemplateStorageMetadata>> {
    let keyword = keyword.trim().to_lowercase();
    let all = ctx.storage.trigger_template.list(None).await?;
    Ok(all
        .into_iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&keyword)
                || t.description
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&keyword))
                    .unwrap_or(false)
        })
        .collect())
}

/// Register or overwrite a trigger template.
pub async fn save(ctx: &ApiContext, template: &TriggerTemplateStorageMetadata) -> ApiResult<()> {
    ctx.storage.trigger_template.save(template).await?;
    let trigger_template = TriggerTemplate {
        name: template.name.clone(),
        description: template.description.clone(),
        condition: template
            .condition
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        action: template
            .action_config
            .as_ref()
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        enabled: Some(template.enabled),
        max_triggers: template.max_triggers,
        priority: template.priority,
        metadata: None,
        created_at: template.created_at,
        updated_at: template.updated_at,
        create_checkpoint: None,
        checkpoint_description_template: None,
    };
    ctx.registries
        .trigger_templates
        .register_or_replace(template.name.clone(), Arc::new(trigger_template));
    Ok(())
}

pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<TriggerTemplateStorageMetadata> {
    ctx.storage
        .trigger_template
        .load(id)
        .await?
        .ok_or_else(|| not_found("trigger_template", id))
}

pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    crate::infra::reference::delete_with_reference_check(
        ctx,
        crate::infra::reference::ReferenceKind::Trigger,
        id,
        false,
    )
    .await
}

/// Export a template by name as a JSON string.
pub async fn export_template(ctx: &ApiContext, name: &str) -> ApiResult<String> {
    crate::template::export_by_name(&ctx.storage.trigger_template, name, "trigger_template").await
}

/// Import a template from a JSON string; returns the imported id.
pub async fn import_template(ctx: &ApiContext, json: &str) -> ApiResult<String> {
    let template: TriggerTemplateStorageMetadata = crate::template::parse_import(json)?;
    save(ctx, &template).await?;
    Ok(template.id.to_string())
}

/// Classify a trigger template by the shape of its persisted condition:
/// `schedule` when no condition is attached, `event` when the condition
/// carries an event type, `condition` for a pure expression condition.
///
/// Single canonical classification shared with the trigger template
/// builder; the `event_type` key is accepted in both the serialized
/// `TriggerCondition` form (`event_type`) and imported form (`eventType`).
pub fn trigger_type_of(condition: Option<&serde_json::Value>) -> &'static str {
    let Some(condition) = condition else {
        return "schedule";
    };
    let event_type = condition
        .get("eventType")
        .or_else(|| condition.get("event_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event_type.is_empty() {
        "condition"
    } else {
        "event"
    }
}

/// Infer the action type from the serialized action config's type:
/// `pause` / `stop` / `checkpoint` for the recognized action types,
/// `custom` otherwise.
pub fn infer_action_type(template: &TriggerTemplateStorageMetadata) -> &'static str {
    let action_type = template
        .action_config
        .as_ref()
        .and_then(|a| a.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match action_type {
        "pause_workflow_execution" => "pause",
        "stop_workflow_execution" | "cancel_workflow_execution" => "stop",
        "create_checkpoint" => "checkpoint",
        _ => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    fn make_template(id: &str, trigger_type: &str) -> TriggerTemplateStorageMetadata {
        TriggerTemplateStorageMetadata {
            id: id.into(),
            name: format!("template-{id}"),
            trigger_type: trigger_type.into(),
            description: Some(format!("desc {id}")),
            category: Some("infra".into()),
            tags: Some(vec!["tag-a".into()]),
            enabled: true,
            max_triggers: Some(10),
            priority: Some(1),
            condition: None,
            action_config: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn query_by_type_category_tags_and_search() {
        let ctx = make_ctx();
        save(&ctx, &make_template("tt-1", "event")).await.unwrap();
        save(&ctx, &make_template("tt-2", "schedule"))
            .await
            .unwrap();

        let all = query(&ctx, None).await.unwrap();
        assert_eq!(all.len(), 2);

        let events = query_by_type(&ctx, "event").await.unwrap();
        assert_eq!(events.len(), 1);

        let infra = query_by_category(&ctx, "infra").await.unwrap();
        assert_eq!(infra.len(), 2);

        let tagged = query_by_tags(&ctx, &["tag-a".to_string()]).await.unwrap();
        assert_eq!(tagged.len(), 2);

        let matches = search(&ctx, "desc tt-1").await.unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[tokio::test]
    async fn summaries_get_delete_import_export() {
        let ctx = make_ctx();
        save(&ctx, &make_template("tt-1", "event")).await.unwrap();

        let summaries = summaries(&ctx, None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].trigger_type, "event");

        let loaded = get(&ctx, "tt-1").await.unwrap();
        assert_eq!(loaded.name, "template-tt-1");

        let exported = export_template(&ctx, "template-tt-1").await.unwrap();
        let imported_id = import_template(&ctx, &exported).await.unwrap();
        assert_eq!(imported_id, "tt-1");

        assert!(delete(&ctx, "tt-1").await.unwrap());
        assert!(!delete(&ctx, "tt-1").await.unwrap());
    }

    #[tokio::test]
    async fn save_registers_template_in_memory() {
        let ctx = make_ctx();
        let template = make_template("tt-memory", "event");

        save(&ctx, &template).await.unwrap();

        assert!(ctx.registries.trigger_templates.has(&template.name));
        assert_eq!(
            ctx.registries
                .trigger_templates
                .get(&template.name)
                .unwrap()
                .name,
            template.name
        );
    }

    #[test]
    fn trigger_type_classification() {
        assert_eq!(trigger_type_of(None), "schedule");

        assert_eq!(
            trigger_type_of(Some(&serde_json::json!({"eventType": "MESSAGE_RECEIVED"}))),
            "event"
        );
        assert_eq!(
            trigger_type_of(Some(&serde_json::json!({"event_type": "AGENT_STARTED"}))),
            "event"
        );
        assert_eq!(
            trigger_type_of(Some(&serde_json::json!({"eventType": ""}))),
            "condition"
        );
        assert_eq!(
            trigger_type_of(Some(&serde_json::json!({"condition": "x > 1"}))),
            "condition"
        );
        let mut pause = make_template("tt-pause", "event");
        pause.action_config = Some(serde_json::json!({"type": "pause_workflow_execution"}));
        assert_eq!(infer_action_type(&pause), "pause");
        let mut stop = make_template("tt-stop", "event");
        stop.action_config = Some(serde_json::json!({"type": "stop_workflow_execution"}));
        assert_eq!(infer_action_type(&stop), "stop");
        let mut cancel = make_template("tt-cancel", "event");
        cancel.action_config = Some(serde_json::json!({"type": "cancel_workflow_execution"}));
        assert_eq!(infer_action_type(&cancel), "stop");

        let mut checkpoint = make_template("tt-cp", "event");
        checkpoint.action_config = Some(serde_json::json!({"type": "create_checkpoint"}));
        assert_eq!(infer_action_type(&checkpoint), "checkpoint");

        let mut custom = make_template("tt-custom", "event");
        custom.action_config = Some(serde_json::json!({"type": "notify_team"}));
        assert_eq!(infer_action_type(&custom), "custom");

        let bare = make_template("tt-bare2", "event");
        assert_eq!(infer_action_type(&bare), "custom");
    }
}
