//! Reverse dependency index and update impact reporting.
//!
//! The index scans stored workflows (and agent templates) for references to
//! shared resources. It backs update-time impact checks and one-click
//! audits. Delete protection in `reference.rs` stays unchanged; this module
//! extends coverage to profiles, sub-workflows and triggers.

use std::collections::HashSet;

use serde::Serialize;
use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;

/// Resource kinds tracked by the reverse index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Tool,
    Script,
    Profile,
    SubWorkflow,
    Trigger,
}

impl DependencyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DependencyKind::Tool => "tool",
            DependencyKind::Script => "script",
            DependencyKind::Profile => "profile",
            DependencyKind::SubWorkflow => "sub_workflow",
            DependencyKind::Trigger => "trigger",
        }
    }
}

/// One dependent location referencing a shared resource.
#[derive(Debug, Clone, Serialize)]
pub struct DependentEntry {
    pub workflow_id: String,
    pub workflow_name: String,
    /// `(workflow-level)` for workflow-level tool lists,
    /// `(agent-template)` for agent template references, otherwise node id.
    pub node_id: String,
    pub field: String,
}

/// Revalidation outcome for one dependent workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactLevel {
    Pass,
    Warning,
    Error,
}

/// Per-dependent revalidation result.
#[derive(Debug, Clone, Serialize)]
pub struct DependentImpact {
    pub workflow_id: String,
    pub workflow_name: String,
    pub node_id: String,
    pub field: String,
    pub level: ImpactLevel,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Impact report for updating a shared resource.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateImpactReport {
    pub resource_kind: DependencyKind,
    pub resource_id: String,
    pub dependents: Vec<DependentImpact>,
    pub error_count: usize,
    pub warning_count: usize,
    pub pass_count: usize,
}

impl UpdateImpactReport {
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }
}

fn config_string(config: &serde_json::Value, key: &str) -> Option<String> {
    config
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
}

fn config_string_array(config: &serde_json::Value, key: &str) -> Vec<String> {
    config
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn inline_agent_config(node: &wf_types::node::BaseStaticNode) -> Option<&serde_json::Value> {
    node.config
        .as_ref()?
        .get("inline_definition")?
        .get("config")
}

fn node_matches(
    node: &wf_types::node::BaseStaticNode,
    kind: DependencyKind,
    id: &str,
) -> Vec<(String, String)> {
    let mut hits = Vec::new();
    let Some(config) = node.config.as_ref() else {
        return hits;
    };
    let node_field = |key: &str| format!("nodes.{}.config.{}", node.id, key);
    match kind {
        DependencyKind::Profile => {
            if config_string(config, "profile_id").as_deref() == Some(id) {
                hits.push((node.id.clone(), node_field("profile_id")));
            }
            if let Some(inline) = inline_agent_config(node) {
                if config_string(inline, "profile_id").as_deref() == Some(id) {
                    hits.push((
                        node.id.clone(),
                        format!(
                            "nodes.{}.config.inline_definition.config.profile_id",
                            node.id
                        ),
                    ));
                }
            }
        }
        DependencyKind::Tool => {
            for name in config_string_array(config, "tool_ids") {
                if name == id {
                    hits.push((node.id.clone(), node_field("tool_ids")));
                }
            }
            for key in ["tool_id", "tool_name"] {
                if config_string(config, key).as_deref() == Some(id) {
                    hits.push((node.id.clone(), node_field(key)));
                }
            }
            if let Some(inline) = inline_agent_config(node) {
                if let Some(tools) = inline.get("available_tools") {
                    for list_key in ["available", "initial", "discoverable", "hidden"] {
                        if let Some(arr) = tools.get(list_key).and_then(|v| v.as_array()) {
                            for entry in arr {
                                if entry.as_str() == Some(id) {
                                    hits.push((
                                        node.id.clone(),
                                        format!(
                                            "nodes.{}.config.inline_definition.config.available_tools.{}",
                                            node.id, list_key
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        DependencyKind::Script => {
            if config_string(config, "script_name").as_deref() == Some(id) {
                hits.push((node.id.clone(), node_field("script_name")));
            }
        }
        DependencyKind::SubWorkflow => {
            for key in ["subgraph_id", "embed_id"] {
                if config_string(config, key).as_deref() == Some(id) {
                    hits.push((node.id.clone(), node_field(key)));
                }
            }
        }
        DependencyKind::Trigger => {
            for key in ["trigger_id", "trigger_template_id", "triggered_workflow_id"] {
                if config_string(config, key).as_deref() == Some(id) {
                    hits.push((node.id.clone(), node_field(key)));
                }
            }
        }
    }
    hits
}

fn workflow_level_tool_hits(
    workflow: &wf_types::WorkflowDefinition,
    id: &str,
) -> Vec<(String, String)> {
    let mut hits = Vec::new();
    if let Some(tools) = workflow.available_tools.as_ref() {
        let empty: &[String] = &[];
        let lists: [(&str, &[String]); 4] = [
            ("available", tools.available.as_slice()),
            ("initial", tools.initial.as_deref().unwrap_or(empty)),
            (
                "discoverable",
                tools.discoverable.as_deref().unwrap_or(empty),
            ),
            ("hidden", tools.hidden.as_deref().unwrap_or(empty)),
        ];
        for (key, names) in lists {
            if names.iter().any(|n| n == id) {
                hits.push((
                    "(workflow-level)".to_string(),
                    format!("workflow.{}.available_tools.{}", workflow.id, key),
                ));
                break;
            }
        }
    }
    hits
}

/// Find all workflow locations referencing `resource_id` of `kind`.
pub async fn find_dependents(
    ctx: &ApiContext,
    kind: DependencyKind,
    resource_id: &str,
) -> crate::ApiResult<Vec<DependentEntry>> {
    let workflows = ctx.storage.workflow.list(None).await?;
    let mut out = Vec::new();
    let candidates = resolve_candidates(ctx, kind, resource_id).await;
    for workflow in &workflows {
        if kind == DependencyKind::Tool {
            for candidate in &candidates {
                for (node_id, field) in workflow_level_tool_hits(workflow, candidate) {
                    out.push(DependentEntry {
                        workflow_id: workflow.id.to_string(),
                        workflow_name: workflow.name.clone(),
                        node_id,
                        field,
                    });
                }
            }
            let workflow_hit = out
                .iter()
                .any(|e| e.workflow_id == workflow.id && e.node_id == "(workflow-level)");
            if workflow_hit {
                continue;
            }
        }
        for node in &workflow.nodes {
            for candidate in &candidates {
                for (node_id, field) in node_matches(node, kind, candidate) {
                    out.push(DependentEntry {
                        workflow_id: workflow.id.to_string(),
                        workflow_name: workflow.name.clone(),
                        node_id,
                        field,
                    });
                }
            }
        }
    }
    if matches!(kind, DependencyKind::Profile | DependencyKind::Tool) {
        for agent_id in ctx.registries.agent_templates.list() {
            if let Some(template) = ctx.registries.agent_templates.get(&agent_id) {
                for candidate in &candidates {
                    if agent_matches(&template.definition, kind, candidate) {
                        out.push(DependentEntry {
                            workflow_id: format!("agent:{}", template.id),
                            workflow_name: template.name.clone(),
                            node_id: "(agent-template)".to_string(),
                            field: format!("agent.{}.config", template.id),
                        });
                        break;
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| {
        a.workflow_id
            .cmp(&b.workflow_id)
            .then(a.node_id.cmp(&b.node_id))
            .then(a.field.cmp(&b.field))
    });
    out.dedup_by(|a, b| {
        a.workflow_id == b.workflow_id && a.node_id == b.node_id && a.field == b.field
    });
    Ok(out)
}

async fn resolve_candidates(
    ctx: &ApiContext,
    kind: DependencyKind,
    resource_id: &str,
) -> Vec<String> {
    let mut candidates = vec![resource_id.to_string()];
    match kind {
        DependencyKind::Tool => {
            if let Ok(Some(meta)) = ctx.storage.tool.load(resource_id).await {
                if meta.tool_id.as_str() != resource_id {
                    candidates.push(meta.tool_id);
                }
                if meta.id.as_str() != resource_id {
                    candidates.push(meta.id);
                }
            }
        }
        DependencyKind::Script => {
            if let Ok(Some(meta)) = ctx.storage.script.load(resource_id).await {
                if meta.name.as_str() != resource_id {
                    candidates.push(meta.name);
                }
                if meta.id.as_str() != resource_id {
                    candidates.push(meta.id);
                }
            }
        }
        DependencyKind::Trigger => {
            if let Ok(Some(meta)) = ctx.storage.trigger_template.load(resource_id).await {
                if meta.name.as_str() != resource_id {
                    candidates.push(meta.name);
                }
                if meta.id.as_str() != resource_id {
                    candidates.push(meta.id);
                }
            }
        }
        DependencyKind::Profile | DependencyKind::SubWorkflow => {}
    }
    let mut seen = HashSet::new();
    candidates.retain(|c| seen.insert(c.clone()));
    candidates
}

fn agent_matches(
    definition: &wf_types::agent::AgentDefinition,
    kind: DependencyKind,
    id: &str,
) -> bool {
    let Some(config) = definition.config.as_ref() else {
        return false;
    };
    match kind {
        DependencyKind::Profile => config.profile_id.as_deref() == Some(id),
        DependencyKind::Tool => config
            .available_tools
            .as_ref()
            .map(|tools| {
                tools.available.iter().any(|n| n == id)
                    || tools
                        .initial
                        .as_ref()
                        .is_some_and(|v| v.iter().any(|n| n == id))
                    || tools
                        .discoverable
                        .as_ref()
                        .is_some_and(|v| v.iter().any(|n| n == id))
                    || tools
                        .hidden
                        .as_ref()
                        .is_some_and(|v| v.iter().any(|n| n == id))
            })
            .unwrap_or(false),
        DependencyKind::Script | DependencyKind::SubWorkflow | DependencyKind::Trigger => false,
    }
}

/// Revalidate every dependent of `resource_id` and report pass/warn/error.
///
/// Dependents are re-run through formal workflow validation with a fresh
/// reference context. Agent template hits are reported as warnings when
/// their own formal validation fails, since they have no graph to re-run.
pub async fn check_update_impact(
    ctx: &ApiContext,
    kind: DependencyKind,
    resource_id: &str,
) -> crate::ApiResult<UpdateImpactReport> {
    let dependents = find_dependents(ctx, kind, resource_id).await?;
    let mut impacts = Vec::new();
    for entry in &dependents {
        if entry.node_id == "(agent-template)" {
            let agent_id = entry.workflow_id.trim_start_matches("agent:");
            let level = if let Some(template) = ctx.registries.agent_templates.get(agent_id) {
                match crate::agent::agent::validate_agent_definition(ctx, &template.definition) {
                    Ok(warnings) if warnings.is_empty() => ImpactLevel::Pass,
                    Ok(warnings) => {
                        let _ = warnings;
                        ImpactLevel::Warning
                    }
                    Err(e) => {
                        let _ = e;
                        ImpactLevel::Error
                    }
                }
            } else {
                ImpactLevel::Pass
            };
            let (errors, warnings) = match level {
                ImpactLevel::Error => (
                    vec![format!("agent '{}' fails formal validation", agent_id)],
                    Vec::new(),
                ),
                ImpactLevel::Warning => (
                    Vec::new(),
                    vec![format!("agent '{}' has reference warnings", agent_id)],
                ),
                ImpactLevel::Pass => (Vec::new(), Vec::new()),
            };
            impacts.push(DependentImpact {
                workflow_id: entry.workflow_id.clone(),
                workflow_name: entry.workflow_name.clone(),
                node_id: entry.node_id.clone(),
                field: entry.field.clone(),
                level,
                errors,
                warnings,
            });
            continue;
        }
        let workflow = ctx
            .storage
            .workflow
            .load(&entry.workflow_id)
            .await?
            .ok_or_else(|| crate::ApiError::not_found("workflow", &entry.workflow_id))?;
        match crate::workflow::validation::validate_workflow_for_publish(ctx, &workflow).await {
            Ok(warnings) => {
                let level = if warnings.is_empty() {
                    ImpactLevel::Pass
                } else {
                    ImpactLevel::Warning
                };
                impacts.push(DependentImpact {
                    workflow_id: entry.workflow_id.clone(),
                    workflow_name: entry.workflow_name.clone(),
                    node_id: entry.node_id.clone(),
                    field: entry.field.clone(),
                    level,
                    errors: Vec::new(),
                    warnings: warnings
                        .into_iter()
                        .map(|w| format!("{}: {}", w.field, w.message))
                        .collect(),
                });
            }
            Err(crate::ApiError::Validation(detail)) => {
                impacts.push(DependentImpact {
                    workflow_id: entry.workflow_id.clone(),
                    workflow_name: entry.workflow_name.clone(),
                    node_id: entry.node_id.clone(),
                    field: entry.field.clone(),
                    level: ImpactLevel::Error,
                    errors: vec![detail],
                    warnings: Vec::new(),
                });
            }
            Err(e) => return Err(e),
        }
    }
    let error_count = impacts
        .iter()
        .filter(|i| i.level == ImpactLevel::Error)
        .count();
    let warning_count = impacts
        .iter()
        .filter(|i| i.level == ImpactLevel::Warning)
        .count();
    let pass_count = impacts
        .iter()
        .filter(|i| i.level == ImpactLevel::Pass)
        .count();
    for impact in &impacts {
        if impact.level == ImpactLevel::Error && !impact.workflow_id.starts_with("agent:") {
            ctx.mark_stale(&impact.workflow_id);
        }
    }
    Ok(UpdateImpactReport {
        resource_kind: kind,
        resource_id: resource_id.to_string(),
        dependents: impacts,
        error_count,
        warning_count,
        pass_count,
    })
}

/// One-click audit: revalidate every stored workflow formally and report
/// those with errors or warnings.
pub async fn audit_all_workflows(ctx: &ApiContext) -> crate::ApiResult<Vec<DependentImpact>> {
    let workflows = ctx.storage.workflow.list(None).await?;
    let mut out = Vec::new();
    for workflow in &workflows {
        match crate::workflow::validation::validate_workflow_for_publish(ctx, workflow).await {
            Ok(warnings) => {
                if !warnings.is_empty() {
                    out.push(DependentImpact {
                        workflow_id: workflow.id.to_string(),
                        workflow_name: workflow.name.clone(),
                        node_id: "(workflow)".to_string(),
                        field: format!("workflow.{}", workflow.id),
                        level: ImpactLevel::Warning,
                        errors: Vec::new(),
                        warnings: warnings
                            .into_iter()
                            .map(|w| format!("{}: {}", w.field, w.message))
                            .collect(),
                    });
                }
            }
            Err(crate::ApiError::Validation(detail)) => {
                out.push(DependentImpact {
                    workflow_id: workflow.id.to_string(),
                    workflow_name: workflow.name.clone(),
                    node_id: "(workflow)".to_string(),
                    field: format!("workflow.{}", workflow.id),
                    level: ImpactLevel::Error,
                    errors: vec![detail],
                    warnings: Vec::new(),
                });
            }
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// Async revalidation for high-frequency write paths: spawns a background
/// impact check so the update interface does not pay full revalidation cost.
pub fn request_async_revalidation(
    ctx: std::sync::Arc<ApiContext>,
    kind: DependencyKind,
    resource_id: String,
) -> tokio::task::JoinHandle<UpdateImpactReport> {
    tokio::spawn(async move {
        match check_update_impact(&ctx, kind, &resource_id).await {
            Ok(report) => {
                if report.has_errors() {
                    tracing::warn!(
                        resource_kind = %kind.as_str(),
                        resource_id = %resource_id,
                        errors = report.error_count,
                        warnings = report.warning_count,
                        "async revalidation found stale dependents"
                    );
                }
                report
            }
            Err(e) => {
                tracing::warn!(
                    resource_kind = %kind.as_str(),
                    resource_id = %resource_id,
                    error = %e,
                    "async revalidation failed"
                );
                UpdateImpactReport {
                    resource_kind: kind,
                    resource_id,
                    dependents: Vec::new(),
                    error_count: 0,
                    warning_count: 0,
                    pass_count: 0,
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        let profile = wf_types::llm::LlmProfile {
            id: "p1".into(),
            name: "p1".into(),
            provider: wf_types::llm::LlmProvider::OpenaiChat,
            model: "m".into(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        };
        let _ = ctx.llm_gateway.profile_registry().register(profile);
        ctx
    }

    fn workflow_with_profile(id: &str, profile: &str) -> wf_types::WorkflowDefinition {
        wf_types::WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {id}"),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![
                wf_types::node::BaseStaticNode {
                    id: "start".into(),
                    node_type: wf_types::node::StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "llm-1".into(),
                    node_type: wf_types::node::StaticNodeType::Llm,
                    name: Some("llm".into()),
                    description: None,
                    config: Some(serde_json::json!({"profile_id": profile})),
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "end".into(),
                    node_type: wf_types::node::StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                wf_types::workflow::Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "llm-1".into(),
                    r#type: wf_types::workflow::EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
                wf_types::workflow::Edge {
                    id: "e2".into(),
                    source_node_id: "llm-1".into(),
                    target_node_id: "end".into(),
                    r#type: wf_types::workflow::EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                },
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn reverse_index_finds_profile_dependents() {
        let ctx = make_ctx();
        ctx.storage
            .workflow
            .save(&workflow_with_profile("wf-dep", "p1"))
            .await
            .unwrap();
        let deps = find_dependents(&ctx, DependencyKind::Profile, "p1")
            .await
            .unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].workflow_id, "wf-dep");
        assert_eq!(deps[0].node_id, "llm-1");
    }

    #[tokio::test]
    async fn update_impact_reports_pass_for_healthy_dependents() {
        let ctx = make_ctx();
        ctx.storage
            .workflow
            .save(&workflow_with_profile("wf-ok", "p1"))
            .await
            .unwrap();
        let report = check_update_impact(&ctx, DependencyKind::Profile, "p1")
            .await
            .unwrap();
        assert_eq!(report.error_count, 0);
        assert_eq!(report.pass_count, 1);
    }

    #[tokio::test]
    async fn audit_reports_stale_workflow_after_profile_removed() {
        let ctx = make_ctx();
        ctx.storage
            .workflow
            .save(&workflow_with_profile("wf-stale", "ghost"))
            .await
            .unwrap();
        let audit = audit_all_workflows(&ctx).await.unwrap();
        assert!(audit
            .iter()
            .any(|i| i.workflow_id == "wf-stale" && i.level == ImpactLevel::Error));
    }
}
