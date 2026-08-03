use serde::Serialize;
use std::collections::BTreeMap;

use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::ApiContext;
use crate::ApiResult;

/// Resource types searchable by the unified search API.
///
/// Note: the TS implementation also searches an in-memory event registry;
/// Rust does not have a persistent event store yet, so "event" is not
/// included until an event registry exists.
pub const SEARCH_RESOURCE_TYPES: [&str; 5] =
    ["workflow", "execution", "task", "checkpoint", "agent_loop"];

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum results per resource type.
    pub limit_per_type: usize,
    /// Maximum total results.
    pub limit_total: usize,
}

impl SearchOptions {
    fn effective(&self) -> (usize, usize) {
        let per_type = if self.limit_per_type == 0 {
            20
        } else {
            self.limit_per_type
        };
        let total = if self.limit_total == 0 {
            100
        } else {
            self.limit_total
        };
        (per_type, total)
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

/// Unified cross-resource search over workflows, executions, tasks,
/// checkpoints and agent loops (TS `SearchAPI` counterpart).
pub struct Searcher {
    ctx: ApiContext,
}

impl Searcher {
    pub fn new(ctx: ApiContext) -> Self {
        Self { ctx }
    }

    pub async fn search(&self, query: &str, options: &SearchOptions) -> ApiResult<SearchResult> {
        let query = query.trim().to_lowercase();
        let (per_type, total_limit) = options.effective();

        let mut results: Vec<SearchResultItem> = Vec::new();

        results.extend(self.search_workflows(&query, per_type).await?);
        results.extend(self.search_executions(&query, per_type).await?);
        results.extend(self.search_tasks(&query, per_type).await?);
        results.extend(self.search_checkpoints(&query, per_type).await?);
        results.extend(self.search_agent_loops(&query, per_type).await?);

        results.sort_by_key(|item| std::cmp::Reverse(item.score));

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
            query: query.clone(),
            items: results,
            by_type,
            total,
            truncated,
        })
    }

    async fn search_workflows(
        &self,
        query: &str,
        limit: usize,
    ) -> ApiResult<Vec<SearchResultItem>> {
        let mut out = Vec::new();
        for id in self.ctx.registries.workflows.list() {
            let Some(template) = self.ctx.registries.workflows.get(&id) else {
                continue;
            };
            let name = template.definition.name.clone();
            let fields = [id.clone(), name.clone()];
            let score = score(query, &fields);
            if score > 0 {
                out.push(SearchResultItem {
                    id: id.clone(),
                    r#type: "workflow".into(),
                    label: name,
                    score,
                    matches: fields
                        .iter()
                        .filter(|f| f.to_lowercase().contains(query))
                        .cloned()
                        .collect(),
                });
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    async fn search_executions(
        &self,
        query: &str,
        limit: usize,
    ) -> ApiResult<Vec<SearchResultItem>> {
        let entities = self.ctx.storage.workflow_execution.list(None).await?;
        let mut out = Vec::new();
        for entity in entities {
            let status = format!("{:?}", entity.status);
            let fields = [
                entity.id.clone(),
                entity.workflow_id.clone(),
                status.clone(),
            ];
            let score = score(query, &fields);
            if score > 0 {
                out.push(SearchResultItem {
                    id: entity.id.clone(),
                    r#type: "execution".into(),
                    label: format!("{} (workflow {})", entity.id, entity.workflow_id),
                    score,
                    matches: fields
                        .iter()
                        .filter(|f| f.to_lowercase().contains(query))
                        .cloned()
                        .collect(),
                });
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    async fn search_tasks(&self, query: &str, limit: usize) -> ApiResult<Vec<SearchResultItem>> {
        let entities = self.ctx.storage.task.list(None).await?;
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
                    matches: fields
                        .iter()
                        .filter(|f| f.to_lowercase().contains(query))
                        .cloned()
                        .collect(),
                });
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    async fn search_checkpoints(
        &self,
        query: &str,
        limit: usize,
    ) -> ApiResult<Vec<SearchResultItem>> {
        let entities = self.ctx.storage.checkpoint.list(None).await?;
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
                    matches: fields
                        .iter()
                        .filter(|f| f.to_lowercase().contains(query))
                        .cloned()
                        .collect(),
                });
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    async fn search_agent_loops(
        &self,
        query: &str,
        limit: usize,
    ) -> ApiResult<Vec<SearchResultItem>> {
        let entities = self.ctx.storage.agent_loop.list(None).await?;
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
                    matches: fields
                        .iter()
                        .filter(|f| f.to_lowercase().contains(query))
                        .cloned()
                        .collect(),
                });
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }
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
    use wf_types::ExecutionStatus;

    #[test]
    fn test_score() {
        assert_eq!(score("abc", &["abc".into(), "zzz".into()]), 2);
        assert_eq!(score("zzz", &["abc".into(), "xzzzy".into()]), 1);
        assert_eq!(score("none", &["abc".into(), "zzz".into()]), 0);
    }

    #[tokio::test]
    async fn test_search_finds_workflows_and_executions() {
        use wf_core::registry::MutableRegistry;

        let storage = StorageContext::new_memory();
        let registries = Arc::new(Registries::new());
        wf_resource::register_all(
            &registries,
            &Arc::new(BundleRegistry::new()),
            &ResourceOptions::default(),
        );

        // Register a workflow whose name matches the query.
        let template = wf_types::workflow::WorkflowTemplate {
            id: "wf-target".to_string(),
            name: "target-flow".to_string(),
            description: "A test workflow".to_string(),
            definition: wf_types::workflow::WorkflowDefinition {
                id: "wf-target".to_string(),
                name: "target-flow".to_string(),
                description: None,
                r#type: None,
                version: None,
                nodes: vec![],
                edges: vec![],
                config: None,
                variables: None,
                triggers: None,
                triggered_subworkflow_config: None,
                metadata: None,
                available_tools: None,
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            },
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        };
        registries
            .workflows
            .register("wf-target".to_string(), Arc::new(template))
            .expect("register workflow");

        // Seed one execution whose id matches the query.
        let execution = wf_types::WorkflowExecution {
            id: "exec-target-1".into(),
            workflow_id: "wf-1".into(),
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
        };
        storage.workflow_execution.save(&execution).await.unwrap();

        let ctx = ApiContext::new(storage, registries, Arc::new(BundleRegistry::new()));
        let searcher = Searcher::new(ctx);

        let result = searcher
            .search("target", &SearchOptions::default())
            .await
            .unwrap();

        assert!(result.items.iter().any(|i| i.r#type == "execution"));
        assert!(result.by_type.contains_key("execution"));
        assert!(result.items.iter().any(|i| i.r#type == "workflow"));
        assert_eq!(result.query, "target");
    }

    #[tokio::test]
    async fn test_search_limit_and_truncation() {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(Registries::new());
        wf_resource::register_all(
            &registries,
            &Arc::new(BundleRegistry::new()),
            &ResourceOptions::default(),
        );

        for i in 0..30 {
            let task = wf_types::TaskStorageMetadata {
                id: format!("task-match-{:02}", i),
                task_type: "search".into(),
                status: "pending".into(),
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            };
            storage.task.save(&task).await.unwrap();
        }

        let ctx = ApiContext::new(storage, registries, Arc::new(BundleRegistry::new()));
        let searcher = Searcher::new(ctx);

        let result = searcher
            .search(
                "match",
                &SearchOptions {
                    limit_per_type: 5,
                    limit_total: 3,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.total, 3);
        assert!(result.truncated);
    }
}
