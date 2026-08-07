//! Template library management (TS `WorkflowTemplateRegistryAPI` /
//! `AgentTemplateRegistryAPI` counterparts).
//!
//! Reads the shared `wf-resource` registries (predefined + custom templates)
//! and tracks usage counts in-memory on the shared context.

use std::sync::Arc;

use serde::Serialize;

use wf_core::registry::{MutableRegistry, Registry};
use wf_types::agent::AgentTemplate;
use wf_types::workflow::WorkflowTemplate;

use crate::context::ApiContext;
use crate::error::{not_found, ApiError, ApiResult};

/// Default number of featured / popular templates returned when no explicit
/// limit is given.
const DEFAULT_FEATURED_LIMIT: usize = 10;

/// Template kind addressed by the template library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKind {
    Workflow,
    Agent,
}

impl TemplateKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TemplateKind::Workflow => "workflow",
            TemplateKind::Agent => "agent",
        }
    }
}

/// Uniform view over both workflow and agent templates (TS
/// `WorkflowTemplateSummary` / `AgentTemplateSummary` counterparts).
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

/// Template library filter (mirrors the TS `WorkflowTemplateFilter`).
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

pub fn delete_workflow_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.registries
        .workflows
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| not_found("workflow_template", id))
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

pub fn register_agent_template(ctx: &ApiContext, template: &AgentTemplate) -> ApiResult<()> {
    let registries = &ctx.registries;
    if registries.agent_templates.has(&template.id) {
        return Err(ApiError::already_exists(
            "agent_template",
            &template.id.to_string(),
        ));
    }
    registries
        .agent_templates
        .register(template.id.to_string(), Arc::new(template.clone()))
        .map_err(|e| ApiError::Conflict(e.to_string()))?;
    Ok(())
}

pub fn delete_agent_template(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    ctx.registries
        .agent_templates
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| not_found("agent_template", id))
}

// ── query ───────────────────────────────────────────────────────

/// Query templates across both kinds with name / category / tags /
/// author filters.
pub fn query(ctx: &ApiContext, filter: &TemplateFilter) -> ApiResult<Vec<TemplateSummary>> {
    let mut summaries = Vec::new();
    let include_workflow = filter.kind.is_none_or(|k| k == TemplateKind::Workflow);
    let include_agent = filter.kind.is_none_or(|k| k == TemplateKind::Agent);

    if include_workflow {
        summaries.extend(list_workflow_templates(ctx)?.into_iter().map(|t| {
            let usage = usage_count(ctx, &t.id.to_string());
            summary_from_workflow(t, usage)
        }));
    }
    if include_agent {
        summaries.extend(list_agent_templates(ctx)?.into_iter().map(|t| {
            let usage = usage_count(ctx, &t.id.to_string());
            summary_from_agent(t, usage)
        }));
    }

    Ok(summaries
        .into_iter()
        .filter(|s| {
            if let Some(name) = &filter.name {
                let name_lower = name.to_lowercase();
                if !s.name.to_lowercase().contains(&name_lower) {
                    return false;
                }
            }
            if let Some(category) = &filter.category {
                if s.category.as_deref() != Some(category.as_str()) {
                    return false;
                }
            }
            if let Some(tags) = &filter.tags {
                if !tags.is_empty() {
                    let has_any = s
                        .tags
                        .as_ref()
                        .map(|t| tags.iter().any(|tag| t.iter().any(|v| v == tag)))
                        .unwrap_or(false);
                    if !has_any {
                        return false;
                    }
                }
            }
            if let Some(author) = &filter.author {
                if s.author.as_deref() != Some(author.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect())
}

pub fn query_by_category(ctx: &ApiContext, category: &str) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            category: Some(category.to_string()),
            ..TemplateFilter::default()
        },
    )
}

pub fn query_by_tags(ctx: &ApiContext, tags: &[String]) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            tags: Some(tags.to_vec()),
            ..TemplateFilter::default()
        },
    )
}

pub fn query_by_author(ctx: &ApiContext, author: &str) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            author: Some(author.to_string()),
            ..TemplateFilter::default()
        },
    )
}

/// Featured templates: public + enabled, most used first.
pub fn featured(ctx: &ApiContext, limit: Option<usize>) -> ApiResult<Vec<TemplateSummary>> {
    let mut all = query(ctx, &TemplateFilter::default())?
        .into_iter()
        .filter(|t| t.is_public && t.enabled)
        .collect::<Vec<_>>();
    all.sort_by_key(|t| std::cmp::Reverse(t.usage_count));
    all.truncate(limit.unwrap_or(DEFAULT_FEATURED_LIMIT));
    Ok(all)
}

/// Popular templates within a category, most used first.
pub fn popular_in_category(
    ctx: &ApiContext,
    category: &str,
    limit: Option<usize>,
) -> ApiResult<Vec<TemplateSummary>> {
    let mut all = query_by_category(ctx, category)?
        .into_iter()
        .filter(|t| t.enabled)
        .collect::<Vec<_>>();
    all.sort_by_key(|t| std::cmp::Reverse(t.usage_count));
    all.truncate(limit.unwrap_or(DEFAULT_FEATURED_LIMIT));
    Ok(all)
}

// ── usage tracking ──────────────────────────────────────────────

/// Increment the usage counter of a template (any kind).
pub fn record_usage(ctx: &ApiContext, id: &str) {
    *ctx.template_usage.entry(id.to_string()).or_insert(0) += 1;
}

pub fn usage_count(ctx: &ApiContext, id: &str) -> u64 {
    ctx.template_usage
        .get(id)
        .map(|e| *e.value())
        .unwrap_or(0)
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
pub fn clone_agent_template(
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
    register_agent_template(ctx, &cloned)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::{register_item, Registries};
    use wf_resource::starter::BundleRegistry;
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
                triggers: None,
                triggered_subworkflow_config: None,
                metadata: Some(WorkflowMetadata {
                    author: Some("system".into()),
                    tags: Some(vec!["tag-a".into()]),
                    category: Some(category.into()),
                }),
                available_tools: None,
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
        let registries = Arc::new(Registries::new());
        register_item(
            &registries.workflows,
            "wf-a".into(),
            workflow_template("wf-a", "analytics"),
            true,
        );
        register_item(
            &registries.workflows,
            "wf-b".into(),
            workflow_template("wf-b", "writing"),
            true,
        );
        register_item(
            &registries.agent_templates,
            "agent-a".into(),
            agent_template("agent-a", "analytics"),
            true,
        );
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            registries,
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[test]
    fn query_filters_and_kind() {
        let ctx = make_ctx();

        let all = query(&ctx, &TemplateFilter::default()).unwrap();
        assert_eq!(all.len(), 3);

        let by_category = query_by_category(&ctx, "analytics").unwrap();
        assert_eq!(by_category.len(), 2);
        assert!(by_category.iter().all(|t| t.kind != "writing"));

        let by_author = query_by_author(&ctx, "author-x").unwrap();
        assert_eq!(by_author.len(), 1);
        assert_eq!(by_author[0].kind, "agent");

        let by_tags = query_by_tags(&ctx, &["tag-b".to_string()]).unwrap();
        assert_eq!(by_tags.len(), 1);

        let by_name = query(
            &ctx,
            &TemplateFilter {
                name: Some("wf".into()),
                ..TemplateFilter::default()
            },
        )
        .unwrap();
        assert_eq!(by_name.len(), 2);

        let workflows_only = query(
            &ctx,
            &TemplateFilter {
                kind: Some(TemplateKind::Workflow),
                ..TemplateFilter::default()
            },
        )
        .unwrap();
        assert_eq!(workflows_only.len(), 2);
    }

    #[test]
    fn featured_and_usage_tracking() {
        let ctx = make_ctx();

        record_usage(&ctx, "wf-b");
        record_usage(&ctx, "wf-b");
        record_usage(&ctx, "wf-a");

        let featured = featured(&ctx, Some(10)).unwrap();
        assert_eq!(featured[0].id, "wf-b");
        assert_eq!(featured[0].usage_count, 2);

        let popular = popular_in_category(&ctx, "analytics", Some(10)).unwrap();
        assert_eq!(popular.len(), 2);
        assert_eq!(popular[0].id, "wf-a");
    }

    #[test]
    fn clone_registers_and_get() {
        let ctx = make_ctx();

        let cloned = clone_workflow_template(&ctx, "wf-a", "My Clone").unwrap();
        assert_ne!(cloned.id.to_string(), "wf-a");
        assert_eq!(cloned.name, "My Clone");
        assert!(ctx.registries.workflows.has(&cloned.id));

        let cloned_agent = clone_agent_template(&ctx, "agent-a", "Agent Clone").unwrap();
        assert!(ctx.registries.agent_templates.has(&cloned_agent.id));

        let err = get_workflow_template(&ctx, "missing").unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));
    }
}
