use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use wf_types::llm::ToolCallFormat;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow_execution::{WorkflowGraphStructure, WorkflowNode};
use wf_types::ValidationError;

/// Maximum recursion depth for subgraph reference closure.
pub const MAX_REFERENCE_DEPTH: usize = 16;

/// Definition-time reference context assembled by the application layer from
/// live registry snapshots. All sets contain known-good identifiers; any
/// reference outside the sets is dangling.
#[derive(Debug, Clone, Default)]
pub struct ReferenceContext {
    pub profile_ids: HashSet<String>,
    pub profile_formats: HashMap<String, ToolCallFormat>,
    pub tool_names: HashSet<String>,
    pub disabled_tools: HashSet<String>,
    pub script_names: HashSet<String>,
    pub workflow_ids: HashSet<String>,
    pub workflow_graphs: HashMap<String, WorkflowGraphStructure>,
    pub trigger_ids: HashSet<String>,
    pub trigger_templates: Vec<TriggerTemplate>,
}

impl ReferenceContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_profile(mut self, id: impl Into<String>, format: Option<ToolCallFormat>) -> Self {
        let id = id.into();
        self.profile_ids.insert(id.clone());
        if let Some(format) = format {
            self.profile_formats.insert(id, format);
        }
        self
    }

    pub fn with_tool(mut self, name: impl Into<String>, enabled: bool) -> Self {
        let name = name.into();
        self.tool_names.insert(name.clone());
        if !enabled {
            self.disabled_tools.insert(name);
        }
        self
    }

    pub fn with_script(mut self, name: impl Into<String>) -> Self {
        self.script_names.insert(name.into());
        self
    }

    pub fn with_workflow(
        mut self,
        id: impl Into<String>,
        graph: Option<WorkflowGraphStructure>,
    ) -> Self {
        let id = id.into();
        self.workflow_ids.insert(id.clone());
        if let Some(graph) = graph {
            self.workflow_graphs.insert(id, graph);
        }
        self
    }

    pub fn with_trigger(mut self, id: impl Into<String>) -> Self {
        self.trigger_ids.insert(id.into());
        self
    }

    pub fn with_trigger_template(mut self, template: TriggerTemplate) -> Self {
        self.trigger_templates.push(template);
        self
    }

    pub fn tool_exists(&self, name: &str) -> bool {
        self.tool_names.contains(name)
    }

    pub fn tool_enabled(&self, name: &str) -> bool {
        self.tool_exists(name) && !self.disabled_tools.contains(name)
    }
}

/// Reference closure report with error/warning separation.
///
/// Hard reference misses block formal registration; weak risks allow
/// registration with a report.
#[derive(Debug, Clone, Default)]
pub struct ReferenceClosureReport {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ReferenceClosureReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn extend(&mut self, other: Self) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

fn error(field: impl Into<String>, message: impl Into<String>) -> ValidationError {
    ValidationError::new(field, message)
}

/// Validate external reference closure of a workflow graph.
///
/// Covers profile existence and format compatibility, tool existence with
/// enabled-state distinction, script existence, subgraph and embed target
/// existence with recursive validation, trigger target existence, and the
/// pure-id agent loop rejection. Graph-internal instance checks for route
/// and fork targets live in core validation.
pub fn validate_reference_closure(
    graph: &WorkflowGraphStructure,
    ctx: &ReferenceContext,
) -> ReferenceClosureReport {
    let mut visited = HashSet::new();
    validate_reference_closure_inner(graph, ctx, 0, &mut visited)
}

fn validate_reference_closure_inner(
    graph: &WorkflowGraphStructure,
    ctx: &ReferenceContext,
    depth: usize,
    visited: &mut HashSet<String>,
) -> ReferenceClosureReport {
    let mut report = ReferenceClosureReport::default();
    for node in &graph.nodes {
        validate_node_references(node, ctx, &mut report);
        validate_subgraph_recursion(node, ctx, depth, visited, &mut report);
    }
    for template in &ctx.trigger_templates {
        validate_trigger_action_references(template, ctx, &mut report);
    }
    report
}

fn node_profile_id(node: &WorkflowNode) -> Option<String> {
    match node.node_type.as_str() {
        "LLM" => node.inner.get("profile_id").and_then(|v| v.as_str()),
        "AGENT_LOOP" => node
            .inner
            .get("inline_definition")
            .and_then(|v| v.get("config"))
            .and_then(|v| v.get("profile_id"))
            .and_then(|v| v.as_str()),
        _ => None,
    }
    .map(String::from)
}

fn node_tool_call_format(node: &WorkflowNode) -> Option<String> {
    let inner = match node.node_type.as_str() {
        "LLM" => node.inner.get("tool_call_format"),
        "AGENT_LOOP" => node
            .inner
            .get("inline_definition")
            .and_then(|v| v.get("config"))
            .and_then(|v| v.get("tool_call_format")),
        _ => return None,
    };
    inner
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
}

fn inline_tool_names(node: &WorkflowNode) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if node.node_type != "AGENT_LOOP" {
        return out;
    }
    let Some(config) = node
        .inner
        .get("inline_definition")
        .and_then(|v| v.get("config"))
    else {
        return out;
    };
    let Some(tools) = config.get("available_tools") else {
        return out;
    };
    for key in ["available", "initial", "discoverable", "hidden"] {
        if let Some(list) = tools.get(key).and_then(|v| v.as_array()) {
            for entry in list {
                if let Some(name) = entry.as_str().filter(|s| !s.trim().is_empty()) {
                    out.push((
                        format!(
                            "nodes.{}.config.inline_definition.config.available_tools.{}",
                            node.id, key
                        ),
                        name.to_string(),
                    ));
                }
            }
        }
    }
    out
}

fn validate_node_references(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    validate_profile_reference(node, ctx, report);
    validate_profile_format_compatibility(node, ctx, report);
    validate_tool_references(node, ctx, report);
    validate_script_reference(node, ctx, report);
    validate_subgraph_existence(node, ctx, report);
    validate_embed_existence(node, ctx, report);
    validate_trigger_reference(node, ctx, report);
    validate_agent_loop_shape(node, report);
}

fn validate_profile_reference(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    let Some(profile_id) = node_profile_id(node) else {
        return;
    };
    if profile_id.trim().is_empty() {
        return;
    }
    if !ctx.profile_ids.contains(&profile_id) {
        report.errors.push(error(
            format!("nodes.{}.config.profile_id", node.id),
            format!(
                "Node '{}' ({}) references profile '{}' which is not registered",
                node.id, node.node_type, profile_id
            ),
        ));
    }
}

fn validate_profile_format_compatibility(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    let Some(profile_id) = node_profile_id(node) else {
        return;
    };
    let Some(format_str) = node_tool_call_format(node) else {
        return;
    };
    let Ok(node_format) = ToolCallFormat::from_str(&format_str) else {
        return;
    };
    let Some(profile_format) = ctx.profile_formats.get(&profile_id) else {
        return;
    };
    if node_format == *profile_format {
        return;
    }
    if node_format.is_compatible_with(profile_format) {
        report.warnings.push(error(
            format!("nodes.{}.config.tool_call_format", node.id),
            format!(
                "Node '{}' tool call format \"{}\" differs from profile '{}' format \"{}\" but both are JSON based",
                node.id, node_format, profile_id, profile_format
            ),
        ));
    } else {
        report.errors.push(error(
            format!("nodes.{}.config.tool_call_format", node.id),
            format!(
                "Node '{}' tool call format \"{}\" is incompatible with profile '{}' format \"{}\"",
                node.id, node_format, profile_id, profile_format
            ),
        ));
    }
}

fn check_tool_name(
    field: String,
    node_id: &str,
    name: &str,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    if name.trim().is_empty() {
        return;
    }
    if !ctx.tool_exists(name) {
        report.errors.push(error(
            field,
            format!(
                "Node '{}' references tool '{}' which is not registered",
                node_id, name
            ),
        ));
    } else if !ctx.tool_enabled(name) {
        report.warnings.push(error(
            field,
            format!(
                "Node '{}' references tool '{}' which is registered but disabled",
                node_id, name
            ),
        ));
    }
}

fn validate_tool_references(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    if node.node_type == "TOOL_VISIBILITY" {
        if let Some(ids) = node.inner.get("tool_ids").and_then(|v| v.as_array()) {
            for (idx, entry) in ids.iter().enumerate() {
                if let Some(name) = entry.as_str() {
                    check_tool_name(
                        format!("nodes.{}.config.tool_ids[{}]", node.id, idx),
                        &node.id,
                        name,
                        ctx,
                        report,
                    );
                }
            }
        }
    }
    for (field, name) in inline_tool_names(node) {
        check_tool_name(field, &node.id, &name, ctx, report);
    }
    for key in ["tool_id", "tool_name"] {
        if let Some(name) = node.inner.get(key).and_then(|v| v.as_str()) {
            check_tool_name(
                format!("nodes.{}.config.{}", node.id, key),
                &node.id,
                name,
                ctx,
                report,
            );
        }
    }
}

/// Validate workflow-level available tool lists against the registry.
pub fn validate_workflow_tool_lists(
    workflow_id: &str,
    available: Option<&wf_types::tool::AvailableTools>,
    ctx: &ReferenceContext,
) -> ReferenceClosureReport {
    let mut report = ReferenceClosureReport::default();
    let Some(tools) = available else {
        return report;
    };
    let empty: &[String] = &[];
    let lists: [(&str, &[String]); 4] = [
        ("available", &tools.available),
        ("initial", tools.initial.as_deref().unwrap_or(empty)),
        (
            "discoverable",
            tools.discoverable.as_deref().unwrap_or(empty),
        ),
        ("hidden", tools.hidden.as_deref().unwrap_or(empty)),
    ];
    for (key, names) in lists {
        for name in names {
            if name.trim().is_empty() {
                continue;
            }
            let field = format!("workflow.{}.available_tools.{}", workflow_id, key);
            if !ctx.tool_exists(name) {
                report.errors.push(error(
                    field,
                    format!(
                        "Workflow '{}' references tool '{}' which is not registered",
                        workflow_id, name
                    ),
                ));
            } else if !ctx.tool_enabled(name) {
                report.warnings.push(error(
                    field,
                    format!(
                        "Workflow '{}' references tool '{}' which is registered but disabled",
                        workflow_id, name
                    ),
                ));
            }
        }
    }
    report
}

fn validate_script_reference(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    if node.node_type != "SCRIPT" && node.node_type != "INTERACTIVE_SCRIPT" {
        return;
    }
    let Some(name) = node.inner.get("script_name").and_then(|v| v.as_str()) else {
        return;
    };
    if name.trim().is_empty() {
        return;
    }
    if !ctx.script_names.contains(name) {
        report.errors.push(error(
            format!("nodes.{}.config.script_name", node.id),
            format!(
                "Node '{}' ({}) references script '{}' which is not registered",
                node.id, node.node_type, name
            ),
        ));
    }
}

fn subgraph_target_id(node: &WorkflowNode) -> Option<String> {
    if node.node_type != "SUBGRAPH" {
        return None;
    }
    node.inner
        .get("subgraph_id")
        .or_else(|| node.inner.get("embed_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
}

fn validate_subgraph_existence(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    let Some(target) = subgraph_target_id(node) else {
        return;
    };
    if !ctx.workflow_ids.contains(&target) {
        report.errors.push(error(
            format!("nodes.{}.config.subgraph_id", node.id),
            format!(
                "Node '{}' (SUBGRAPH) references workflow '{}' which is not registered",
                node.id, target
            ),
        ));
    }
}

fn validate_embed_existence(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    if node.node_type != "EMBED_GRAPH" {
        return;
    }
    let has_inline = node
        .inner
        .get("graph_definition")
        .is_some_and(|v| !v.is_null());
    if has_inline {
        return;
    }
    let Some(embed_id) = node
        .inner
        .get("embed_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        return;
    };
    if !ctx.workflow_ids.contains(embed_id) {
        report.errors.push(error(
            format!("nodes.{}.config.embed_id", node.id),
            format!(
                "Node '{}' (EMBED_GRAPH) references embed '{}' which is not registered",
                node.id, embed_id
            ),
        ));
    }
}

fn validate_trigger_reference(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    if ctx.trigger_ids.is_empty() {
        return;
    }
    for key in ["trigger_id", "trigger_template_id", "triggered_workflow_id"] {
        if let Some(id) = node
            .inner
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            let is_workflow_key = key == "triggered_workflow_id";
            let known = if is_workflow_key {
                ctx.workflow_ids.contains(id)
            } else {
                ctx.trigger_ids.contains(id)
            };
            if !known {
                report.errors.push(error(
                    format!("nodes.{}.config.{}", node.id, key),
                    format!(
                        "Node '{}' references {} '{}' which is not registered",
                        node.id, key, id
                    ),
                ));
            }
        }
    }
}

fn validate_trigger_action_references(
    template: &TriggerTemplate,
    ctx: &ReferenceContext,
    report: &mut ReferenceClosureReport,
) {
    let Some(ref action) = template.action else {
        return;
    };
    match action {
        wf_types::trigger::TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id,
            ..
        } => {
            if !ctx.workflow_ids.contains(triggered_workflow_id) {
                report.errors.push(error(
                    format!("trigger.{}.action.triggered_workflow_id", template.name),
                    format!(
                        "Trigger '{}' references workflow '{}' which is not registered",
                        template.name, triggered_workflow_id
                    ),
                ));
            }
        }
        wf_types::trigger::TriggerAction::ExecuteScript { script_name, .. } => {
            if !ctx.script_names.contains(script_name) {
                report.errors.push(error(
                    format!("trigger.{}.action.script_name", template.name),
                    format!(
                        "Trigger '{}' references script '{}' which is not registered",
                        template.name, script_name
                    ),
                ));
            }
        }
        wf_types::trigger::TriggerAction::ExecuteTriggeredAgentExecution {
            model: Some(profile),
            ..
        } => {
            if !ctx.profile_ids.contains(profile) {
                report.errors.push(error(
                    format!("trigger.{}.action.model", template.name),
                    format!(
                        "Trigger '{}' references profile '{}' which is not registered",
                        template.name, profile
                    ),
                ));
            }
        }
        wf_types::trigger::TriggerAction::ExecuteTriggeredAgentExecution {
            model: None, ..
        } => {}
        _ => {}
    }
}

fn validate_agent_loop_shape(node: &WorkflowNode, report: &mut ReferenceClosureReport) {
    if node.node_type != "AGENT_LOOP" {
        return;
    }
    let has_loop_id = node
        .inner
        .get("agent_loop_id")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());
    let has_inline = node
        .inner
        .get("inline_definition")
        .is_some_and(|v| !v.is_null());
    if has_loop_id && !has_inline {
        let loop_id = node
            .inner
            .get("agent_loop_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        report.errors.push(error(
            format!("nodes.{}.config.agent_loop_id", node.id),
            format!(
                "AGENT_LOOP node '{}' references agent '{}' by id only, which has no runtime resolution. Provide inline_definition with config.profile_id and available_tools, or migrate the referenced agent into an inline definition",
                node.id, loop_id
            ),
        ));
    }
}

fn validate_subgraph_recursion(
    node: &WorkflowNode,
    ctx: &ReferenceContext,
    depth: usize,
    visited: &mut HashSet<String>,
    report: &mut ReferenceClosureReport,
) {
    let target = if node.node_type == "SUBGRAPH" {
        subgraph_target_id(node)
    } else if node.node_type == "EMBED_GRAPH" {
        let has_inline = node
            .inner
            .get("graph_definition")
            .is_some_and(|v| !v.is_null());
        if has_inline {
            None
        } else {
            node.inner
                .get("embed_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(String::from)
        }
    } else {
        None
    };
    let Some(target) = target else {
        return;
    };
    if visited.contains(&target) {
        report.errors.push(error(
            format!("nodes.{}.config.subgraph_id", node.id),
            format!(
                "Node '{}' introduces a circular subgraph reference through '{}'",
                node.id, target
            ),
        ));
        return;
    }
    if depth >= MAX_REFERENCE_DEPTH {
        report.errors.push(error(
            format!("nodes.{}.config.subgraph_id", node.id),
            format!(
                "Node '{}' exceeds the maximum subgraph reference depth ({}) through '{}'",
                node.id, MAX_REFERENCE_DEPTH, target
            ),
        ));
        return;
    }
    let Some(target_graph) = ctx.workflow_graphs.get(&target) else {
        return;
    };
    visited.insert(target.clone());
    let nested = validate_reference_closure_inner(target_graph, ctx, depth + 1, visited);
    visited.remove(&target);
    for err in nested.errors {
        report.errors.push(error(
            format!("nodes.{}.subgraph[{}].{}", node.id, target, err.field),
            err.message,
        ));
    }
    for warn in nested.warnings {
        report.warnings.push(error(
            format!("nodes.{}.subgraph[{}].{}", node.id, target, warn.field),
            warn.message,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowNode};

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn graph_with(nodes: Vec<WorkflowNode>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges: Vec::<WorkflowEdge>::new(),
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: None,
            end_node_ids: Vec::new(),
        }
    }

    #[test]
    fn missing_profile_is_error() {
        let ctx = ReferenceContext::new().with_profile("known", None);
        let graph = graph_with(vec![node(
            "l1",
            "LLM",
            serde_json::json!({"profile_id": "ghost"}),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("ghost"));
    }

    #[test]
    fn missing_tool_is_error_and_disabled_is_warning() {
        let ctx = ReferenceContext::new()
            .with_tool("live_tool", true)
            .with_tool("old_tool", false);
        let graph = graph_with(vec![
            node(
                "t1",
                "TOOL_VISIBILITY",
                serde_json::json!({"tool_ids": ["ghost_tool"]}),
            ),
            node(
                "t2",
                "TOOL_VISIBILITY",
                serde_json::json!({"tool_ids": ["old_tool"]}),
            ),
        ]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn missing_script_is_error() {
        let ctx = ReferenceContext::new().with_script("known_script");
        let graph = graph_with(vec![node(
            "s1",
            "SCRIPT",
            serde_json::json!({"script_name": "ghost", "risk": "low"}),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn missing_subgraph_is_error() {
        let ctx = ReferenceContext::new();
        let graph = graph_with(vec![node(
            "sub",
            "SUBGRAPH",
            serde_json::json!({"subgraph_id": "ghost-wf"}),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn pure_agent_id_is_rejected() {
        let ctx = ReferenceContext::new();
        let graph = graph_with(vec![node(
            "a1",
            "AGENT_LOOP",
            serde_json::json!({"agent_loop_id": "loop-1"}),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("inline_definition"));
    }

    #[test]
    fn circular_subgraph_is_rejected() {
        let inner = graph_with(vec![node(
            "sub",
            "SUBGRAPH",
            serde_json::json!({"subgraph_id": "wf-a"}),
        )]);
        let ctx = ReferenceContext::new().with_workflow("wf-a", Some(inner));
        let graph = graph_with(vec![node(
            "sub",
            "SUBGRAPH",
            serde_json::json!({"subgraph_id": "wf-a"}),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert!(report.errors.iter().any(|e| e.message.contains("circular")));
    }

    #[test]
    fn inline_agent_tool_lists_are_checked() {
        let ctx = ReferenceContext::new()
            .with_profile("mock", None)
            .with_tool("live_tool", true)
            .with_tool("old_tool", false);
        let graph = graph_with(vec![node(
            "a1",
            "AGENT_LOOP",
            serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {
                        "profile_id": "mock",
                        "available_tools": {
                            "available": ["ghost_tool"],
                            "initial": ["old_tool"],
                            "hidden": ["live_tool"],
                        },
                    },
                },
            }),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("ghost_tool"));
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].message.contains("old_tool"));
    }

    #[test]
    fn inline_agent_profile_must_be_registered() {
        let ctx = ReferenceContext::new().with_profile("known", None);
        let graph = graph_with(vec![node(
            "a1",
            "AGENT_LOOP",
            serde_json::json!({
                "inline_definition": {
                    "id": "a1",
                    "name": "agent",
                    "config": {"profile_id": "ghost"},
                },
            }),
        )]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("ghost"));
    }

    #[test]
    fn incompatible_format_is_error_and_json_diff_is_warning() {
        let ctx = ReferenceContext::new()
            .with_profile("xml-p", Some(ToolCallFormat::Xml))
            .with_profile("json-p", Some(ToolCallFormat::JsonWrapped));
        let graph = graph_with(vec![
            node(
                "l1",
                "LLM",
                serde_json::json!({"profile_id": "xml-p", "tool_call_format": "native"}),
            ),
            node(
                "l2",
                "LLM",
                serde_json::json!({"profile_id": "json-p", "tool_call_format": "json_raw"}),
            ),
        ]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.warnings.len(), 1);
    }

    fn make_trigger_template(
        name: &str,
        action: wf_types::trigger::TriggerAction,
    ) -> TriggerTemplate {
        TriggerTemplate {
            name: name.to_string(),
            description: None,
            condition: None,
            action: Some(action),
            enabled: Some(true),
            max_triggers: None,
            priority: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    #[test]
    fn trigger_action_workflow_reference_valid() {
        let ctx = ReferenceContext::new()
            .with_workflow("my-workflow", None)
            .with_trigger_template(make_trigger_template(
                "t1",
                wf_types::trigger::TriggerAction::ExecuteTriggeredSubworkflow {
                    triggered_workflow_id: "my-workflow".to_string(),
                    wait_for_completion: None,
                    timeout: None,
                    input_mapping: None,
                    output_mapping: None,
                },
            ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn trigger_action_workflow_reference_invalid() {
        let ctx = ReferenceContext::new().with_trigger_template(make_trigger_template(
            "t1",
            wf_types::trigger::TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id: "ghost-wf".to_string(),
                wait_for_completion: None,
                timeout: None,
                input_mapping: None,
                output_mapping: None,
            },
        ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("ghost-wf"));
        assert!(report.errors[0].message.contains("workflow"));
    }

    #[test]
    fn trigger_action_script_reference_valid() {
        let ctx = ReferenceContext::new()
            .with_script("my-script")
            .with_trigger_template(make_trigger_template(
                "t1",
                wf_types::trigger::TriggerAction::ExecuteScript {
                    script_name: "my-script".to_string(),
                    parameters: None,
                    timeout: None,
                    ignore_error: None,
                },
            ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn trigger_action_script_reference_invalid() {
        let ctx = ReferenceContext::new().with_trigger_template(make_trigger_template(
            "t1",
            wf_types::trigger::TriggerAction::ExecuteScript {
                script_name: "ghost-script".to_string(),
                parameters: None,
                timeout: None,
                ignore_error: None,
            },
        ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("ghost-script"));
        assert!(report.errors[0].message.contains("script"));
    }

    #[test]
    fn trigger_action_profile_reference_valid() {
        let ctx = ReferenceContext::new()
            .with_profile("my-profile", None)
            .with_trigger_template(make_trigger_template(
                "t1",
                wf_types::trigger::TriggerAction::ExecuteTriggeredAgentExecution {
                    agent_id: "child-agent".to_string(),
                    prompt: None,
                    model: Some("my-profile".to_string()),
                    result_variable: None,
                    wait_for_completion: None,
                    timeout: None,
                    input_mode: None,
                    writeback: None,
                },
            ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn trigger_action_profile_reference_invalid() {
        let ctx = ReferenceContext::new().with_trigger_template(make_trigger_template(
            "t1",
            wf_types::trigger::TriggerAction::ExecuteTriggeredAgentExecution {
                agent_id: "child-agent".to_string(),
                prompt: None,
                model: Some("ghost-profile".to_string()),
                result_variable: None,
                wait_for_completion: None,
                timeout: None,
                input_mode: None,
                writeback: None,
            },
        ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("ghost-profile"));
        assert!(report.errors[0].message.contains("profile"));
    }

    #[test]
    fn trigger_action_no_action_skipped() {
        let ctx = ReferenceContext::new().with_trigger_template(TriggerTemplate {
            name: "t1".to_string(),
            description: None,
            condition: None,
            action: None,
            enabled: Some(true),
            max_triggers: None,
            priority: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        });
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn trigger_action_other_variants_skipped() {
        let ctx = ReferenceContext::new().with_trigger_template(make_trigger_template(
            "t1",
            wf_types::trigger::TriggerAction::StopWorkflowExecution {},
        ));
        let graph = graph_with(vec![]);
        let report = validate_reference_closure(&graph, &ctx);
        assert!(report.errors.is_empty());
    }
}
