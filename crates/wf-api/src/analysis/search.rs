//! Unified cross-resource search over workflows, executions, tasks,
//! checkpoints, events and agent loops.
//!
//! Types are searched in parallel; a failure of one resource type degrades to
//! empty results for that type instead of failing the whole search. Results
//! are collected first and then sorted, so the outcome never depends on
//! registry iteration order.

use serde::Serialize;
use std::collections::BTreeMap;

use futures::future::join_all;

use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::ApiContext;
use crate::ApiResult;

/// Resource types searchable by the unified search API (TS `SearchAPI`
/// counterpart). `Event` searches the most recent events retained by the
/// shared `EventBus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchResourceType {
    Workflow,
    Execution,
    Task,
    Checkpoint,
    Event,
    AgentLoop,
}

impl SearchResourceType {
    /// Canonical snake_case name of the resource type.
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchResourceType::Workflow => "workflow",
            SearchResourceType::Execution => "execution",
            SearchResourceType::Task => "task",
            SearchResourceType::Checkpoint => "checkpoint",
            SearchResourceType::Event => "event",
            SearchResourceType::AgentLoop => "agent_loop",
        }
    }

    /// All searchable resource types, in a stable order.
    pub fn all() -> [SearchResourceType; 6] {
        [
            SearchResourceType::Workflow,
            SearchResourceType::Execution,
            SearchResourceType::Task,
            SearchResourceType::Checkpoint,
            SearchResourceType::Event,
            SearchResourceType::AgentLoop,
        ]
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Resource types to search; `None` searches every type.
    pub types: Option<Vec<SearchResourceType>>,
    /// Maximum results per resource type; `None` uses the default.
    pub limit_per_type: Option<usize>,
    /// Maximum total results; `None` uses the default.
    pub limit_total: Option<usize>,
}

/// Default maximum results per resource type when no explicit limit is given.
const DEFAULT_LIMIT_PER_TYPE: usize = 20;
/// Default maximum total results when no explicit limit is given.
const DEFAULT_LIMIT_TOTAL: usize = 100;

impl SearchOptions {
    fn effective(&self) -> (Vec<SearchResourceType>, usize, usize) {
        let types = self
            .types
            .clone()
            .unwrap_or_else(|| SearchResourceType::all().to_vec());
        let per_type = self.limit_per_type.unwrap_or(DEFAULT_LIMIT_PER_TYPE);
        let total = self.limit_total.unwrap_or(DEFAULT_LIMIT_TOTAL);
        (types, per_type, total)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultItem {
    pub id: String,
    pub r#type: String,
    pub label: String,
    pub score: u32,
    pub matches: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub query: String,
    pub items: Vec<SearchResultItem>,
    pub by_type: BTreeMap<String, Vec<SearchResultItem>>,
    pub total: usize,
    pub truncated: bool,
}

pub async fn search(
    ctx: &ApiContext,
    query: &str,
    options: &SearchOptions,
) -> ApiResult<SearchResult> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(SearchResult {
            query,
            items: Vec::new(),
            by_type: BTreeMap::new(),
            total: 0,
            truncated: false,
        });
    }
    let (types, per_type, total_limit) = options.effective();

    let futures = types
        .iter()
        .copied()
        .map(|resource_type| {
            let query_ref = &query;
            async move {
                let result = search_type(ctx, query_ref, resource_type, per_type).await;
                match result {
                    Ok(items) => items,
                    Err(err) => {
                        tracing::warn!(
                            target: "wf_api",
                            resource_type = resource_type.as_str(),
                            error = %err,
                            "resource search failed, degrading to empty results"
                        );
                        Vec::new()
                    }
                }
            }
        })
        .collect::<Vec<_>>();

    let mut results: Vec<SearchResultItem> =
        join_all(futures).await.into_iter().flatten().collect();
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.r#type.cmp(&b.r#type))
            .then_with(|| a.id.cmp(&b.id))
    });

    let truncated = results.len() > total_limit;
    results.truncate(total_limit);

    let mut by_type: BTreeMap<String, Vec<SearchResultItem>> = BTreeMap::new();
    for item in &results {
        by_type
            .entry(item.r#type.clone())
            .or_default()
            .push(item.clone());
    }

    let total = results.len();
    Ok(SearchResult {
        query,
        items: results,
        by_type,
        total,
        truncated,
    })
}

async fn search_type(
    ctx: &ApiContext,
    query: &str,
    resource_type: SearchResourceType,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    match resource_type {
        SearchResourceType::Workflow => search_workflows(ctx, query, limit).await,
        SearchResourceType::Execution => search_executions(ctx, query, limit).await,
        SearchResourceType::Task => search_tasks(ctx, query, limit).await,
        SearchResourceType::Checkpoint => search_checkpoints(ctx, query, limit).await,
        SearchResourceType::Event => Ok(search_events(ctx, query, limit).await),
        SearchResourceType::AgentLoop => search_agent_loops(ctx, query, limit).await,
    }
}

async fn search_workflows(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let mut out = Vec::new();
    for id in ctx.registries.workflows.list() {
        let Some(template) = ctx.registries.workflows.get(&id) else {
            continue;
        };

        let mut fields = vec![id.clone(), template.name.clone()];
        if !template.description.is_empty() {
            fields.push(template.description.clone());
        }
        if let Some(desc) = &template.definition.description {
            fields.push(desc.clone());
        }
        for tag in workflow_tags(&template) {
            fields.push(tag);
        }

        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: id.clone(),
                r#type: "workflow".into(),
                label: template.name.clone(),
                score,
                matches: matched_fields(query, &fields),
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}

async fn search_executions(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.workflow_execution.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.workflow_id.clone(),
            entity.status.as_str().to_string(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "execution".into(),
                label: format!("{} (workflow {})", entity.id, entity.workflow_id),
                score,
                matches: matched_fields(query, &fields),
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}

async fn search_tasks(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.task.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.task_type.clone(),
            entity.status.clone(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "task".into(),
                label: format!("{} ({})", entity.task_type, entity.status),
                score,
                matches: matched_fields(query, &fields),
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}

async fn search_checkpoints(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.checkpoint.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.entity_id.clone(),
            entity.entity_type.clone(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "checkpoint".into(),
                label: format!("{} (entity {})", entity.id, entity.entity_id),
                score,
                matches: matched_fields(query, &fields),
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}

/// Search the recent events retained by the shared event bus.
async fn search_events(ctx: &ApiContext, query: &str, limit: usize) -> Vec<SearchResultItem> {
    let mut out = Vec::new();
    for event in ctx.event_bus.recent_events() {
        let type_name = event.r#type.as_str().to_lowercase();
        let mut fields = vec![type_name.clone(), event.id.clone()];
        if let Some(workflow_id) = &event.workflow_id {
            fields.push(workflow_id.clone());
        }
        if let Some(execution_id) = &event.execution_id {
            fields.push(execution_id.clone());
        }
        if let Some(agent_loop_id) = &event.agent_loop_id {
            fields.push(agent_loop_id.clone());
        }

        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: event.id.clone(),
                r#type: "event".into(),
                label: format!("Event {}", event.r#type.as_str()),
                score,
                matches: matched_fields(query, &fields),
            });
        }
    }
    sorted_truncated(out, limit)
}

async fn search_agent_loops(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.agent_loop.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.definition_id.clone(),
            entity.status.clone(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "agent_loop".into(),
                label: format!("{} (definition {})", entity.id, entity.definition_id),
                score,
                matches: matched_fields(query, &fields),
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}

/// Tags of a workflow template, from both template-level and definition-level
/// metadata, deduplicated and in stable order.
fn workflow_tags(template: &wf_types::workflow::WorkflowTemplate) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    if let Some(ts) = &template.template_tags {
        tags.extend(ts.iter().cloned());
    }
    if let Some(metadata) = &template.definition.metadata {
        if let Some(ts) = &metadata.tags {
            tags.extend(ts.iter().cloned());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

/// Sort by relevance (descending) then id (ascending) and truncate.
fn sorted_truncated(mut items: Vec<SearchResultItem>, limit: usize) -> Vec<SearchResultItem> {
    items.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    items.truncate(limit);
    items
}

/// Field values matching the query, in field order.
fn matched_fields(query: &str, fields: &[String]) -> Vec<String> {
    fields
        .iter()
        .filter(|f| f.to_lowercase().contains(query))
        .cloned()
        .collect()
}

/// Simple substring scoring: +2 per matching id, +1 per other matching field.
fn score(query: &str, fields: &[String]) -> u32 {
    if query.is_empty() {
        return 0;
    }
    let mut total = 0;
    for (idx, field) in fields.iter().enumerate() {
        if field.to_lowercase().contains(query) {
            total += if idx == 0 { 2 } else { 1 };
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registrar::{Options as ResourceOptions, Registries};
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::events::{BaseEvent, EventType};
    use wf_types::workflow::{WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
    use wf_types::ExecutionStatus;

    fn make_ctx() -> (Arc<Registries>, Arc<BundleRegistry>, Arc<ApiContext>) {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(Registries::new());
        wf_resource::register_all(
            &registries,
            &Arc::new(BundleRegistry::new()),
            &ResourceOptions::default(),
        );
        let bundles = Arc::new(BundleRegistry::new());
        let ctx = Arc::new(ApiContext::new(
            storage,
            registries.clone(),
            bundles.clone(),
        ));
        (registries, bundles, ctx)
    }

    fn ctx_only() -> Arc<ApiContext> {
        let (_, _, ctx) = make_ctx();
        ctx
    }

    fn make_workflow_template(id: &str, name: &str, tags: Option<Vec<String>>) -> WorkflowTemplate {
        WorkflowTemplate {
            id: id.to_string(),
            name: name.to_string(),
            description: "A test workflow".to_string(),
            definition: WorkflowDefinition {
                id: id.to_string(),
                name: name.to_string(),
                description: Some("desc-target".to_string()),
                r#type: None,
                version: None,
                nodes: vec![],
                edges: vec![],
                config: None,
                variables: None,
                triggers: None,
                triggered_subworkflow_config: None,
                metadata: Some(WorkflowMetadata {
                    author: None,
                    tags,
                    category: None,
                }),
                available_tools: None,
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            },
            template_category: None,
            template_tags: Some(vec!["blueprint".to_string()]),
            is_public: None,
            enabled: None,
        }
    }

    fn make_execution(id: &str, workflow_id: &str) -> wf_types::WorkflowExecution {
        wf_types::WorkflowExecution {
            id: id.into(),
            workflow_id: workflow_id.into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: wf_common::now(),
            completed_at: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        }
    }

    #[test]
    fn test_score() {
        assert_eq!(score("abc", &["abc".into(), "zzz".into()]), 2);
        assert_eq!(score("zzz", &["abc".into(), "xzzzy".into()]), 1);
        assert_eq!(score("none", &["abc".into(), "zzz".into()]), 0);
    }

    #[tokio::test]
    async fn test_search_finds_workflows_and_executions() {
        use wf_core::registry::MutableRegistry;

        let (registries, _, ctx) = make_ctx();
        registries
            .workflows
            .register(
                "wf-target".to_string(),
                Arc::new(make_workflow_template("wf-target", "target-flow", None)),
            )
            .expect("register workflow");

        ctx.storage
            .workflow_execution
            .save(&make_execution("exec-target-1", "wf-1"))
            .await
            .unwrap();

        let result = search(&ctx, "target", &SearchOptions::default())
            .await
            .unwrap();

        assert!(result.items.iter().any(|i| i.r#type == "execution"));
        assert!(result.by_type.contains_key("execution"));
        assert!(result.items.iter().any(|i| i.r#type == "workflow"));
        assert_eq!(result.query, "target");
    }

    #[tokio::test]
    async fn test_search_filters_by_type() {
        use wf_core::registry::MutableRegistry;

        let (registries, _, ctx) = make_ctx();
        registries
            .workflows
            .register(
                "wf-target".to_string(),
                Arc::new(make_workflow_template("wf-target", "target-flow", None)),
            )
            .expect("register workflow");
        ctx.storage
            .workflow_execution
            .save(&make_execution("exec-target-1", "wf-1"))
            .await
            .unwrap();

        let result = search(
            &ctx,
            "target",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Execution]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();

        assert!(result.items.iter().all(|i| i.r#type == "execution"));
        assert_eq!(result.items.len(), 1);
        assert!(!result.by_type.contains_key("workflow"));
    }

    #[tokio::test]
    async fn test_search_matches_description_and_tags() {
        use wf_core::registry::MutableRegistry;

        let (registries, _, ctx) = make_ctx();
        registries
            .workflows
            .register(
                "wf-tagged".to_string(),
                Arc::new(make_workflow_template(
                    "wf-tagged",
                    "flow-a",
                    Some(vec!["salesforce".to_string()]),
                )),
            )
            .expect("register workflow");

        let by_description = search(
            &ctx,
            "desc-target",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Workflow]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_description.items.len(), 1);

        let by_tag = search(
            &ctx,
            "salesforce",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Workflow]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_tag.items.len(), 1);
    }

    #[tokio::test]
    async fn test_search_events_from_event_bus() {
        let ctx = ctx_only();
        let _sub = ctx.event_bus.subscribe();
        let event = BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::WorkflowExecutionStarted,
            timestamp: wf_common::now(),
            workflow_id: Some("wf-target".to_string()),
            execution_id: Some("exec-target".to_string()),
            agent_loop_id: None,
            metadata: None,
        };
        ctx.event_bus.publish(event).unwrap();

        let result = search(
            &ctx,
            "workflow_execution_started",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Event]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].r#type, "event");
    }

    #[tokio::test]
    async fn test_search_limit_and_truncation() {
        let ctx = ctx_only();
        for i in 0..30 {
            let task = wf_types::TaskStorageMetadata {
                id: format!("task-match-{:02}", i),
                task_type: "search".into(),
                status: "pending".into(),
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            };
            ctx.storage.task.save(&task).await.unwrap();
        }

        let result = search(
            &ctx,
            "match",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Task]),
                limit_per_type: Some(5),
                limit_total: Some(3),
            },
        )
        .await
        .unwrap();
        assert_eq!(result.total, 3);
        assert!(result.truncated);
    }

    #[tokio::test]
    async fn test_search_results_are_deterministic() {
        use wf_core::registry::MutableRegistry;

        let (registries, _, ctx) = make_ctx();
        for i in 0..10 {
            registries
                .workflows
                .register(
                    format!("wf-{i:02}"),
                    Arc::new(make_workflow_template(
                        &format!("wf-{i:02}"),
                        &format!("flow-{i:02}"),
                        None,
                    )),
                )
                .expect("register workflow");
        }

        let options = SearchOptions {
            types: Some(vec![SearchResourceType::Workflow]),
            limit_per_type: Some(5),
            ..SearchOptions::default()
        };
        let first = search(&ctx, "flow", &options).await.unwrap();
        let second = search(&ctx, "flow", &options).await.unwrap();
        let first_ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
        let second_ids: Vec<String> = second.items.iter().map(|i| i.id.clone()).collect();
        assert_eq!(first_ids, second_ids);
    }
}
