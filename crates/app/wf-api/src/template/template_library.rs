//! Template library management.
//!
//! Reads the shared `wf-resource` registries (predefined + custom templates)
//! and tracks usage counts in the persistent `template_usage` store, so
//! featured and popular ordering survives restarts.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

use wf_core::registry::{MutableRegistry, Registry};
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::agent::AgentTemplate;
use wf_types::workflow::WorkflowTemplate;

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// Default number of featured / popular templates returned when no explicit
/// limit is given.
const DEFAULT_FEATURED_LIMIT: usize = 10;

/// Template kind addressed by the template library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    Workflow,
    Agent,
    Node,
    Trigger,
}

impl TemplateKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateKind::Workflow => "workflow",
            TemplateKind::Agent => "agent",
            TemplateKind::Node => "node",
            TemplateKind::Trigger => "trigger",
        }
    }
}

/// Uniform view over both workflow and agent templates.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateSummary {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub is_public: bool,
    pub enabled: bool,
    pub usage_count: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Template library filter.
#[derive(Debug, Clone, Default)]
pub struct TemplateFilter {
    pub kind: Option<TemplateKind>,
    pub name: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
}

// ── workflow templates ──────────────────────────────────────────

pub fn get_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<WorkflowTemplate> {
    ctx.registries
        .workflows
        .get(id)
        .map(|t| t.as_ref().clone())
        .ok_or_else(|| not_found("workflow_template", id))
}

pub fn list_workflow_templates(ctx: &ApiContext) -> ApiResult<Vec<WorkflowTemplate>> {
    let keys = ctx.registries.workflows.list();
    let mut templates = Vec::new();
    for key in keys {
        if let Some(template) = ctx.registries.workflows.get(&key) {
            templates.push(template.as_ref().clone());
        }
    }
    Ok(templates)
}

/// Register a workflow template; errors with `AlreadyExists` when the id
/// is already registered.
pub fn register_workflow_template(ctx: &ApiContext, template: &WorkflowTemplate) -> ApiResult<()> {
    let registries = &ctx.registries;
    if registries.workflows.has(&template.id) {
        return Err(ApiError::already_exists(
            "workflow_template",
            &template.id.to_string(),
        ));
    }
    registries
        .workflows
        .register(template.id.to_string(), Arc::new(template.clone()))
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    Ok(())
}

pub async fn delete_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.registries
        .workflows
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| not_found("workflow_template", id))?;
    let _ = ctx.storage.template_usage.delete(id).await;
    Ok(())
}

/// Replace a workflow template in place; errors with `NotFound` when the
/// id is not registered.
pub fn update_workflow_template(ctx: &ApiContext, template: &WorkflowTemplate) -> ApiResult<()> {
    if !ctx.registries.workflows.has(&template.id) {
        return Err(not_found("workflow_template", &template.id.to_string()));
    }
    ctx.registries
        .workflows
        .register_or_replace(template.id.to_string(), Arc::new(template.clone()));
    Ok(())
}

// ── agent templates ─────────────────────────────────────────────

pub fn get_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<AgentTemplate> {
    ctx.registries
        .agent_templates
        .get(id)
        .map(|t| t.as_ref().clone())
        .ok_or_else(|| not_found("agent_template", id))
}

pub fn list_agent_templates(ctx: &ApiContext) -> ApiResult<Vec<AgentTemplate>> {
    let keys = ctx.registries.agent_templates.list();
    let mut templates = Vec::new();
    for key in keys {
        if let Some(template) = ctx.registries.agent_templates.get(&key) {
            templates.push(template.as_ref().clone());
        }
    }
    Ok(templates)
}

pub async fn register_agent_template(ctx: &ApiContext, template: &AgentTemplate) -> ApiResult<()> {
    let registries = &ctx.registries;
    if registries.agent_templates.has(&template.id) {
        return Err(ApiError::already_exists(
            "agent_template",
            &template.id.to_string(),
        ));
    }
    ctx.storage
        .agent_template
        .save(template)
        .await
        .map_err(crate::ApiError::from)?;
    registries
        .agent_templates
        .register(
            template.id.to_string(),
            std::sync::Arc::new(template.clone()),
        )
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    Ok(())
}

pub async fn delete_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.storage
        .agent_template
        .delete(id)
        .await
        .map_err(crate::ApiError::from)?;
    ctx.registries
        .agent_templates
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| not_found("agent_template", id))?;
    let _ = ctx.storage.template_usage.delete(id).await;
    Ok(())
}

/// Replace an agent template in place; errors with `NotFound` when the id
/// is not registered.
pub async fn update_agent_template(ctx: &ApiContext, template: &AgentTemplate) -> ApiResult<()> {
    if !ctx.registries.agent_templates.has(&template.id) {
        return Err(not_found("agent_template", &template.id.to_string()));
    }
    ctx.storage
        .agent_template
        .save(template)
        .await
        .map_err(crate::ApiError::from)?;
    ctx.registries.agent_templates.register_or_replace(
        template.id.to_string(),
        std::sync::Arc::new(template.clone()),
    );
    Ok(())
}

// ── query ───────────────────────────────────────────────────────

/// Query templates across both kinds with name / category / tags /
/// author filters. Usage counts join from the persistent usage store in one
/// read so featured and popular ordering survives restarts.
pub async fn query(ctx: &ApiContext, filter: &TemplateFilter) -> ApiResult<Vec<TemplateSummary>> {
    let mut summaries = Vec::new();
    let include_workflow = filter.kind.is_none_or(|k| k == TemplateKind::Workflow);
    let include_agent = filter.kind.is_none_or(|k| k == TemplateKind::Agent);
    let usage = usage_map(ctx).await?;

    if include_workflow {
        summaries.extend(list_workflow_templates(ctx)?.into_iter().map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            summary_from_workflow(t, count)
        }));
    }
    if include_agent {
        summaries.extend(list_agent_templates(ctx)?.into_iter().map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            summary_from_agent(t, count)
        }));
    }

    let basic = crate::template::BasicTemplateFilter {
        name: filter.name.clone(),
        category: filter.category.clone(),
        tags: filter.tags.clone(),
        author: filter.author.clone(),
    };

    Ok(summaries
        .into_iter()
        .filter(|s| {
            basic.matches(
                &s.name,
                s.category.as_deref(),
                s.tags.as_deref(),
                s.author.as_deref(),
            )
        })
        .collect())
}

pub async fn query_by_category(
    ctx: &ApiContext,
    category: &str,
) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            category: Some(category.to_string()),
            ..TemplateFilter::default()
        },
    )
    .await
}

pub async fn query_by_tags(ctx: &ApiContext, tags: &[String]) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            tags: Some(tags.to_vec()),
            ..TemplateFilter::default()
        },
    )
    .await
}

pub async fn query_by_author(ctx: &ApiContext, author: &str) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            author: Some(author.to_string()),
            ..TemplateFilter::default()
        },
    )
    .await
}

/// Unified query across all four template kinds. A missing kind means all.
/// Workflow and agent entries come from the in-memory library, node and
/// trigger entries from storage. Keeps the single-request library surface.
pub async fn query_unified(
    ctx: &ApiContext,
    filter: &TemplateFilter,
) -> ApiResult<Vec<TemplateSummary>> {
    let include_workflow = filter.kind.is_none_or(|k| k == TemplateKind::Workflow);
    let include_agent = filter.kind.is_none_or(|k| k == TemplateKind::Agent);
    let include_node = filter.kind.is_none_or(|k| k == TemplateKind::Node);
    let include_trigger = filter.kind.is_none_or(|k| k == TemplateKind::Trigger);

    let mut summaries = Vec::new();
    if include_workflow || include_agent {
        let library_filter = TemplateFilter {
            kind: if include_workflow && include_agent {
                None
            } else if include_workflow {
                Some(TemplateKind::Workflow)
            } else {
                Some(TemplateKind::Agent)
            },
            name: filter.name.clone(),
            category: filter.category.clone(),
            tags: filter.tags.clone(),
            author: filter.author.clone(),
        };
        summaries.extend(query(ctx, &library_filter).await?);
    }
    let basic = crate::template::BasicTemplateFilter {
        name: filter.name.clone(),
        category: filter.category.clone(),
        tags: filter.tags.clone(),
        author: filter.author.clone(),
    };
    if include_node {
        let nodes = ctx.storage.node_template.list(None).await?;
        let usage = usage_map(ctx).await?;
        summaries.extend(nodes.into_iter().filter_map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            let summary = summary_from_node(t, count);
            basic
                .matches(
                    &summary.name,
                    summary.category.as_deref(),
                    summary.tags.as_deref(),
                    summary.author.as_deref(),
                )
                .then_some(summary)
        }));
    }
    if include_trigger {
        let triggers = ctx.storage.trigger_template.list(None).await?;
        let usage = usage_map(ctx).await?;
        summaries.extend(triggers.into_iter().filter_map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            let summary = summary_from_trigger(t, count);
            basic
                .matches(
                    &summary.name,
                    summary.category.as_deref(),
                    summary.tags.as_deref(),
                    summary.author.as_deref(),
                )
                .then_some(summary)
        }));
    }
    Ok(summaries)
}

/// Featured templates: public + enabled, most used first.
pub async fn featured(ctx: &ApiContext, limit: Option<usize>) -> ApiResult<Vec<TemplateSummary>> {
    let mut all = query(ctx, &TemplateFilter::default())
        .await?
        .into_iter()
        .filter(|t| t.is_public && t.enabled)
        .collect::<Vec<_>>();
    all.sort_by_key(|t| std::cmp::Reverse(t.usage_count));
    all.truncate(limit.unwrap_or(DEFAULT_FEATURED_LIMIT));
    Ok(all)
}

/// Popular templates within a category, most used first.
pub async fn popular_in_category(
    ctx: &ApiContext,
    category: &str,
    limit: Option<usize>,
) -> ApiResult<Vec<TemplateSummary>> {
    let mut all = query_by_category(ctx, category)
        .await?
        .into_iter()
        .filter(|t| t.enabled)
        .collect::<Vec<_>>();
    all.sort_by_key(|t| std::cmp::Reverse(t.usage_count));
    all.truncate(limit.unwrap_or(DEFAULT_FEATURED_LIMIT));
    Ok(all)
}

// ── usage tracking ──────────────────────────────────────────────

/// Usage counts keyed by template id, read once per query from the
/// persistent store. Templates without a record count as zero.
async fn usage_map(ctx: &ApiContext) -> ApiResult<HashMap<String, u64>> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let records = ctx.storage.template_usage.list(None).await?;
    Ok(records
        .into_iter()
        .map(|record| (record.template_id, record.count))
        .collect())
}

/// Resolve the stored kind of a template id for first-use record creation.
async fn resolve_usage_kind(ctx: &ApiContext, id: &str) -> String {
    if ctx.registries.workflows.has(id) {
        return TemplateKind::Workflow.as_str().to_string();
    }
    if ctx.registries.agent_templates.has(id) {
        return TemplateKind::Agent.as_str().to_string();
    }
    use wf_storage::adapter::base::BaseStorageAdapter;
    if ctx
        .storage
        .node_template
        .load(id)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return TemplateKind::Node.as_str().to_string();
    }
    if ctx
        .storage
        .trigger_template
        .load(id)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return TemplateKind::Trigger.as_str().to_string();
    }
    TemplateKind::Workflow.as_str().to_string()
}

/// Increment the usage counter of a template (any kind) and return the new
/// count. Called explicitly when a template is instantiated, cloned, or
/// exported; repeated calls each count once.
pub async fn record_usage(ctx: &ApiContext, id: &str) -> ApiResult<u64> {
    use wf_storage::adapter::template_usage::TemplateUsageStorageAdapter;
    let kind = resolve_usage_kind(ctx, id).await;
    ctx.storage
        .template_usage
        .increment(id, &kind)
        .await
        .map_err(crate::ApiError::from)
}

/// Current usage count of a template; zero when never recorded or when the
/// store read fails, so library queries stay robust.
pub async fn usage_count(ctx: &ApiContext, id: &str) -> u64 {
    use wf_storage::adapter::template_usage::TemplateUsageStorageAdapter;
    ctx.storage.template_usage.get_count(id).await.unwrap_or(0)
}

// ── clone ───────────────────────────────────────────────────────

/// Clone a workflow template under a new id/name and register the clone.
pub fn clone_workflow_template(
    ctx: &ApiContext,
    id: &str,
    new_name: &str,
) -> ApiResult<WorkflowTemplate> {
    let template = get_workflow_template(ctx, id)?;
    let mut cloned = template.clone();
    cloned.id = format!("cloned-{}", wf_common::generate_id());
    cloned.name = new_name.to_string();
    cloned.description = if template.description.is_empty() {
        format!("Clone of {}", template.name)
    } else {
        format!("Clone of {}", template.description)
    };
    cloned.definition.id = cloned.id.clone();
    cloned.definition.name = new_name.to_string();
    cloned.definition.created_at = wf_common::now();
    cloned.definition.updated_at = wf_common::now();
    register_workflow_template(ctx, &cloned)?;
    Ok(cloned)
}

/// Clone an agent template under a new id/name and register the clone.
pub async fn clone_agent_template(
    ctx: &ApiContext,
    id: &str,
    new_name: &str,
) -> ApiResult<AgentTemplate> {
    let template = get_agent_template(ctx, id)?;
    let mut cloned = template.clone();
    cloned.id = format!("cloned-{}", wf_common::generate_id());
    cloned.name = new_name.to_string();
    cloned.description = if template.description.is_empty() {
        format!("Clone of {}", template.name)
    } else {
        format!("Clone of {}", template.description)
    };
    cloned.definition.id = cloned.id.clone();
    cloned.definition.name = new_name.to_string();
    cloned.definition.created_at = wf_common::now();
    cloned.definition.updated_at = wf_common::now();
    register_agent_template(ctx, &cloned).await?;
    Ok(cloned)
}

fn summary_from_workflow(template: WorkflowTemplate, usage_count: u64) -> TemplateSummary {
    let author = template
        .definition
        .metadata
        .as_ref()
        .and_then(|m| m.author.clone());
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Workflow.as_str(),
        name: template.name,
        description: template.description,
        category: template.template_category,
        tags: template.template_tags,
        author,
        is_public: template.is_public.unwrap_or(true),
        enabled: template.enabled.unwrap_or(true),
        usage_count,
        created_at: template.definition.created_at,
        updated_at: template.definition.updated_at,
    }
}

fn summary_from_agent(template: AgentTemplate, usage_count: u64) -> TemplateSummary {
    let author = template
        .definition
        .metadata
        .as_ref()
        .and_then(|m| m.author.clone());
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Agent.as_str(),
        name: template.name,
        description: template.description,
        category: template.template_category,
        tags: template.template_tags,
        author,
        is_public: template.is_public.unwrap_or(true),
        enabled: template.enabled.unwrap_or(true),
        usage_count,
        created_at: template.definition.created_at,
        updated_at: template.definition.updated_at,
    }
}

fn summary_from_node(
    template: wf_types::NodeTemplateStorageMetadata,
    usage_count: u64,
) -> TemplateSummary {
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Node.as_str(),
        name: template.name,
        description: template.description.unwrap_or_default(),
        category: None,
        tags: None,
        author: None,
        is_public: true,
        enabled: true,
        usage_count,
        created_at: template.created_at,
        updated_at: template.updated_at,
    }
}

fn summary_from_trigger(
    template: wf_types::TriggerTemplateStorageMetadata,
    usage_count: u64,
) -> TemplateSummary {
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Trigger.as_str(),
        name: template.name,
        description: template.description.unwrap_or_default(),
        category: template.category,
        tags: template.tags,
        author: None,
        is_public: true,
        enabled: template.enabled,
        usage_count,
        created_at: template.created_at,
        updated_at: template.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registry::{register_item_skip, ResourceRegistries};
    use wf_storage::context::StorageContext;
    use wf_types::agent::{AgentDefinition, AgentMetadata};
    use wf_types::workflow::{WorkflowDefinition, WorkflowMetadata};

    fn now() -> i64 {
        wf_common::now()
    }

    fn workflow_template(id: &str, category: &str) -> WorkflowTemplate {
        WorkflowTemplate {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: format!("Description {}", id),
            definition: WorkflowDefinition {
                id: id.into(),
                name: format!("Workflow {}", id),
                description: None,
                r#type: None,
                version: None,
                nodes: vec![],
                edges: vec![],
                config: None,
                variables: None,
                triggered_subworkflow_config: None,
                metadata: Some(WorkflowMetadata {
                    author: Some("system".into()),
                    tags: Some(vec!["tag-a".into()]),
                    category: Some(category.into()),
                }),
                available_tools: None,
                hooks: None,
                created_at: now(),
                updated_at: now(),
            },
            template_category: Some(category.into()),
            template_tags: Some(vec!["tag-a".into()]),
            is_public: Some(true),
            enabled: Some(true),
        }
    }

    fn agent_template(id: &str, category: &str) -> AgentTemplate {
        AgentTemplate {
            id: id.into(),
            name: format!("Agent {}", id),
            description: format!("Description {}", id),
            definition: AgentDefinition {
                id: id.into(),
                name: format!("Agent {}", id),
                description: None,
                version: None,
                config: None,
                metadata: Some(AgentMetadata {
                    author: Some("author-x".into()),
                    tags: Some(vec!["tag-b".into()]),
                    category: Some(category.into()),
                }),
                created_at: now(),
                updated_at: now(),
            },
            template_category: Some(category.into()),
            template_tags: Some(vec!["tag-b".into()]),
            is_public: Some(true),
            enabled: Some(true),
        }
    }

    fn make_ctx() -> Arc<ApiContext> {
        let registries = Arc::new(ResourceRegistries::new());
        register_item_skip(
            &registries.workflows,
            "wf-a".into(),
            workflow_template("wf-a", "analytics"),
        );
        register_item_skip(
            &registries.workflows,
            "wf-b".into(),
            workflow_template("wf-b", "writing"),
        );
        register_item_skip(
            &registries.agent_templates,
            "agent-a".into(),
            agent_template("agent-a", "analytics"),
        );
        Arc::new(ApiContext::new(StorageContext::new_memory(), registries))
    }

    #[tokio::test]
    async fn query_filters_and_kind() {
        let ctx = make_ctx();

        let all = query(&ctx, &TemplateFilter::default()).await.unwrap();
        assert_eq!(all.len(), 3);

        let by_category = query_by_category(&ctx, "analytics").await.unwrap();
        assert_eq!(by_category.len(), 2);
        assert!(by_category.iter().all(|t| t.kind != "writing"));

        let by_author = query_by_author(&ctx, "author-x").await.unwrap();
        assert_eq!(by_author.len(), 1);
        assert_eq!(by_author[0].kind, "agent");

        let by_tags = query_by_tags(&ctx, &["tag-b".to_string()]).await.unwrap();
        assert_eq!(by_tags.len(), 1);

        let by_name = query(
            &ctx,
            &TemplateFilter {
                name: Some("wf".into()),
                ..TemplateFilter::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_name.len(), 2);

        let workflows_only = query(
            &ctx,
            &TemplateFilter {
                kind: Some(TemplateKind::Workflow),
                ..TemplateFilter::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(workflows_only.len(), 2);
    }

    #[tokio::test]
    async fn featured_and_usage_tracking() {
        let ctx = make_ctx();

        record_usage(&ctx, "wf-b").await.unwrap();
        record_usage(&ctx, "wf-b").await.unwrap();
        record_usage(&ctx, "wf-a").await.unwrap();

        let featured = featured(&ctx, Some(10)).await.unwrap();
        assert_eq!(featured[0].id, "wf-b");
        assert_eq!(featured[0].usage_count, 2);

        let popular = popular_in_category(&ctx, "analytics", Some(10))
            .await
            .unwrap();
        assert_eq!(popular.len(), 2);
        assert_eq!(popular[0].id, "wf-a");
    }

    #[tokio::test]
    async fn usage_survives_context_rebuild() {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(ResourceRegistries::new());
        register_item_skip(
            &registries.workflows,
            "wf-a".into(),
            workflow_template("wf-a", "analytics"),
        );
        let ctx = Arc::new(ApiContext::new(storage, registries));
        record_usage(&ctx, "wf-a").await.unwrap();
        assert_eq!(usage_count(&ctx, "wf-a").await, 1);
        assert_eq!(usage_count(&ctx, "missing").await, 0);
    }

    #[tokio::test]
    async fn update_replaces_in_place_and_rejects_missing() {
        let ctx = make_ctx();

        let mut renamed = workflow_template("wf-a", "analytics");
        renamed.name = "Renamed".into();
        update_workflow_template(&ctx, &renamed).unwrap();
        assert_eq!(get_workflow_template(&ctx, "wf-a").unwrap().name, "Renamed");
        let missing = workflow_template("nope", "analytics");
        assert!(matches!(
            update_workflow_template(&ctx, &missing).unwrap_err(),
            ApiError::NotFound { .. }
        ));

        let mut agent_renamed = agent_template("agent-a", "analytics");
        agent_renamed.name = "Renamed Agent".into();
        update_agent_template(&ctx, &agent_renamed).await.unwrap();
        assert_eq!(
            get_agent_template(&ctx, "agent-a").unwrap().name,
            "Renamed Agent"
        );
        let agent_missing = agent_template("nope", "analytics");
        assert!(matches!(
            update_agent_template(&ctx, &agent_missing)
                .await
                .unwrap_err(),
            ApiError::NotFound { .. }
        ));
    }

    #[tokio::test]
    async fn clone_registers_and_get() {
        let ctx = make_ctx();

        let cloned = clone_workflow_template(&ctx, "wf-a", "My Clone").unwrap();
        assert_ne!(cloned.id.to_string(), "wf-a");
        assert_eq!(cloned.name, "My Clone");
        assert!(ctx.registries.workflows.has(&cloned.id));

        let cloned_agent = clone_agent_template(&ctx, "agent-a", "Agent Clone")
            .await
            .unwrap();
        assert!(ctx.registries.agent_templates.has(&cloned_agent.id));

        let err = get_workflow_template(&ctx, "missing").unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }
}
