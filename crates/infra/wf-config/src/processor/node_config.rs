mod agent_loop;
mod common;
mod fork_join;
mod interaction;
mod llm;
mod route;
mod script;
mod subgraph;
mod variable;

pub use common::{is_valid_identifier, NodeConfigIssue};

use serde_json::Value;

use self::agent_loop::validate_agent_loop_node;
use self::fork_join::{validate_fork_node, validate_join_node};
use self::interaction::{validate_tool_visibility_node, validate_user_interaction_node};
use self::llm::validate_llm_node;
use self::route::validate_route_node;
use self::script::validate_script_node;
use self::subgraph::validate_subgraph_node;
use self::variable::validate_variable_node;

/// Validate the config of one workflow node by its node type.
///
/// Builtin node types have a dedicated validator; every other non-empty name
/// is treated as a plugin-contributed type and passes without inspection,
/// because its config schema is only known to the plugin that registered it.
/// `config` is the node's config value (`WorkflowNode.inner` or
/// `BaseStaticNode.config`). Node type names are case-insensitive.
pub fn validate_node_config(
    node_type: &str,
    node_id: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let empty = Value::Object(Default::default());
    let effective: Option<&Value> = match node_type.to_uppercase().as_str() {
        "LLM" | "SCRIPT" | "VARIABLE" => Some(config.unwrap_or(&empty)),
        _ => config,
    };

    match node_type.to_uppercase().as_str() {
        "LLM" => validate_llm_node(node_id, node_type, effective),
        "SCRIPT" => validate_script_node(node_id, node_type, effective),
        "VARIABLE" => validate_variable_node(node_id, node_type, effective),
        "ROUTE" => validate_route_node(node_id, node_type, effective),
        "FORK" => validate_fork_node(node_id, node_type, effective),
        "JOIN" => validate_join_node(node_id, node_type, effective),
        "SUBGRAPH" => validate_subgraph_node(node_id, node_type, effective),
        "USER_INTERACTION" => validate_user_interaction_node(node_id, node_type, effective),
        "AGENT_LOOP" => validate_agent_loop_node(node_id, node_type, effective),
        "TOOL_VISIBILITY" => validate_tool_visibility_node(node_id, node_type, effective),
        "CONTEXT_PROCESSOR" => validate_context_processor_node(node_id, node_type, config),
        "START"
        | "END"
        | "EMBED_START"
        | "EMBED_END"
        | "SYNC"
        | "EMBED_GRAPH"
        | "INTERACTIVE_SCRIPT"
        | "LOOP_START"
        | "LOOP_END"
        | "START_FROM_MESSAGE"
        | "CONTINUE_FROM_MESSAGE" => Vec::new(),
        // Plugin-contributed types have no builtin config schema, so there is
        // nothing to check here; resolution happens when the graph is executed.
        _ => Vec::new(),
    }
}

fn validate_context_processor_node(
    node_id: &str,
    node_type: &str,
    config: Option<&Value>,
) -> Vec<NodeConfigIssue> {
    let Some(config) = config else {
        return Vec::new();
    };
    if config.is_null() {
        return Vec::new();
    }
    let Some(obj) = config.as_object() else {
        return Vec::new();
    };
    match obj.get("fold") {
        None | Some(Value::Null) | Some(Value::Bool(_)) => Vec::new(),
        Some(_) => vec![NodeConfigIssue::new(
            format!("nodes.{node_id}.config.fold"),
            format!("Node '{node_id}' ({node_type}) fold selector must be a boolean"),
        )],
    }
}

/// Validate every node's config and collect all issues.
pub fn validate_all_node_configs(nodes: &[(&str, &str, Option<&Value>)]) -> Vec<NodeConfigIssue> {
    let mut issues = Vec::new();
    for (node_id, node_type, config) in nodes {
        issues.extend(validate_node_config(node_type, node_id, *config));
    }
    issues
}

#[cfg(test)]
#[path = "node_config/tests.rs"]
mod tests;
