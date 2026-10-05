use serde_json::json;

use wf_types::agent::{AgentConfig, AgentDefinition};
use wf_types::node::BaseStaticNode;
use wf_types::node::StaticNodeType;
use wf_types::tool::AvailableTools;
use wf_types::workflow::{WorkflowDefinition, WorkflowDefinitionType, WorkflowTemplate};

use crate::embedded_assets;

use super::super::{edge, workflow_metadata};

pub const PREFETCH_WORKFLOW_ID: &str = "@standard/code-context-prefetch";

pub const PREFETCH_START_NODE_ID: &str = "prefetch-start";
pub const PREFETCH_AGENT_NODE_ID: &str = "prefetch-agent";
pub const PREFETCH_END_NODE_ID: &str = "prefetch-end";

const PREFETCH_AGENT_PROFILE_ID: &str = "DEFAULT";
const PREFETCH_AGENT_MAX_ITERATIONS: u32 = 8;

/// Tools the prefetch agent may call. Read-only code tools only; numeric
/// service keys never cross this boundary because navigation runs on file
/// paths plus line numbers.
pub const PREFETCH_AVAILABLE_TOOLS: &[&str] = &[
    "code_search",
    "code_keyword_search",
    "code_symbols",
    "code_references",
    "code_definition",
    "read_file_folded",
];

pub use crate::predefined::agent_prompts::CODE_CONTEXT_PREFETCH_PROMPT_KEY as PREFETCH_AGENT_SYSTEM_PROMPT_KEY;

pub fn prefetch_inline_definition() -> AgentDefinition {
    prefetch_inline_definition_with_prompt(None)
}

/// Inline prefetch definition with an optional system-prompt override;
/// `None` keeps the embedded default.
pub fn prefetch_inline_definition_with_prompt(system_prompt: Option<String>) -> AgentDefinition {
    let tools = PREFETCH_AVAILABLE_TOOLS
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    AgentDefinition {
        id: "@standard/code-context-prefetch-agent".into(),
        name: "Code Context Prefetch".into(),
        description: Some("Bounded pre-task code evidence collector".into()),
        version: Some("1.0.0".into()),
        config: Some(AgentConfig {
            profile_id: Some(PREFETCH_AGENT_PROFILE_ID.into()),
            system_prompt: Some(system_prompt.unwrap_or_else(|| {
                embedded_assets::agent_prompt(PREFETCH_AGENT_SYSTEM_PROMPT_KEY).to_string()
            })),
            max_iterations: Some(PREFETCH_AGENT_MAX_ITERATIONS),
            max_execution_time: None,
            max_retries: None,
            execution_timeout: None,
            max_pause_duration: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            initial_messages: None,
            available_tools: Some(AvailableTools {
                available: tools,
                initial: None,
                discoverable: None,
                enable_general_tool: Some(false),
                hidden: None,
                require_approval: None,
                allowed_workflows: None,
            }),
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            stream: Some(true),
            tool_call_protocol: Some("native".into()),
            hooks: None,
            dynamic_context: None,
            checkpoint: None,
            violation_policy: None,
        }),
        metadata: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
    }
}

fn start_node() -> BaseStaticNode {
    BaseStaticNode {
        id: PREFETCH_START_NODE_ID.into(),
        node_type: StaticNodeType::Start,
        name: Some("Start Prefetch".into()),
        description: Some(
            "Receive the input object {task, projectId, directoryPrefix, maxResults} from the parent subgraph node"
                .into(),
        ),
        config: None,
        execution_config: None,
    }
}

fn agent_node_with_prompt(system_prompt: Option<String>) -> BaseStaticNode {
    let inline = prefetch_inline_definition_with_prompt(system_prompt);
    let inline = serde_json::to_value(&inline).expect("inline definition serializes");
    BaseStaticNode {
        id: PREFETCH_AGENT_NODE_ID.into(),
        node_type: StaticNodeType::AgentLoop,
        name: Some("Collect Code Evidence".into()),
        description: Some("Bounded agent loop over read-only code tools".into()),
        config: Some(json!({
            "inline_definition": inline,
        })),
        execution_config: None,
    }
}

fn end_node() -> BaseStaticNode {
    BaseStaticNode {
        id: PREFETCH_END_NODE_ID.into(),
        node_type: StaticNodeType::End,
        name: Some("End Prefetch".into()),
        description: Some(
            "Pass the agent's evidence output through as the child workflow output".into(),
        ),
        config: None,
        execution_config: None,
    }
}

pub fn create_prefetch_workflow() -> WorkflowTemplate {
    create_prefetch_workflow_with_prompt(None)
}

/// Prefetch workflow with an optional prefetch-agent system-prompt override;
/// `None` keeps the embedded default.
pub fn create_prefetch_workflow_with_prompt(system_prompt: Option<String>) -> WorkflowTemplate {
    let t = wf_common::now();

    WorkflowTemplate {
        id: PREFETCH_WORKFLOW_ID.into(),
        name: "Code Context Prefetch Workflow".into(),
        description: "Builtin pre-task subgraph: bounded agent loop gathers task-related code evidence and returns it as the subgraph node output".into(),
        definition: WorkflowDefinition {
            id: PREFETCH_WORKFLOW_ID.into(),
            name: "Code Context Prefetch Workflow".into(),
            description: Some(
                "Prefetch-then-work pattern: input object in, evidence list out. Child-only contract: the parent SUBGRAPH node timeout bounds the run and the agent iteration limit always applies"
                    .into(),
            ),
            r#type: Some(WorkflowDefinitionType::Dependent),
            version: Some("1.0.0".into()),
            nodes: vec![start_node(), agent_node_with_prompt(system_prompt), end_node()],
            edges: vec![
                edge(
                    "e-prefetch-start-to-agent",
                    PREFETCH_START_NODE_ID,
                    PREFETCH_AGENT_NODE_ID,
                ),
                edge(
                    "e-prefetch-agent-to-end",
                    PREFETCH_AGENT_NODE_ID,
                    PREFETCH_END_NODE_ID,
                ),
            ],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: Some(workflow_metadata(&[
                "context",
                "prefetch",
                "code",
                "predefined",
            ])),
            available_tools: None,
            created_at: t,
            updated_at: t,
            hooks: None,
        },
        template_category: Some("system".into()),
        template_tags: Some(vec!["context".into(), "prefetch".into(), "code".into()]),
        is_public: Some(true),
        enabled: Some(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_definition_stays_bounded_and_read_only() {
        let inline = prefetch_inline_definition();
        let config = inline.config.expect("config present");
        assert_eq!(
            config.profile_id.as_deref(),
            Some(PREFETCH_AGENT_PROFILE_ID)
        );
        assert_eq!(config.max_iterations, Some(PREFETCH_AGENT_MAX_ITERATIONS));
        let tools = config.available_tools.expect("tools present");
        assert_eq!(tools.available.len(), PREFETCH_AVAILABLE_TOOLS.len());
        assert_eq!(tools.enable_general_tool, Some(false));
    }

    #[test]
    fn template_chains_start_to_agent_to_end() {
        let template = create_prefetch_workflow();
        assert_eq!(template.id, PREFETCH_WORKFLOW_ID);
        assert_eq!(template.definition.nodes.len(), 3);
        assert_eq!(template.definition.edges.len(), 2);
        let agent = template
            .definition
            .nodes
            .iter()
            .find(|node| node.id == PREFETCH_AGENT_NODE_ID)
            .expect("agent node present");
        assert_eq!(agent.node_type, StaticNodeType::AgentLoop);
        let inline = agent
            .config
            .as_ref()
            .and_then(|config| config.get("inline_definition"))
            .expect("inline definition present");
        assert!(inline
            .get("config")
            .and_then(|config| config.get("profile_id"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty()));
    }

    #[test]
    fn start_and_end_are_configless_passthrough_nodes() {
        let template = create_prefetch_workflow();
        let start = template
            .definition
            .nodes
            .iter()
            .find(|node| node.id == PREFETCH_START_NODE_ID)
            .expect("start node present");
        assert_eq!(start.node_type, StaticNodeType::Start);
        assert!(start.config.is_none());

        let end = template
            .definition
            .nodes
            .iter()
            .find(|node| node.id == PREFETCH_END_NODE_ID)
            .expect("end node present");
        assert_eq!(end.node_type, StaticNodeType::End);
        assert!(end.config.is_none());
    }

    #[test]
    fn prompt_reads_the_input_object_contract() {
        let inline = prefetch_inline_definition();
        let config = inline.config.expect("config present");
        let prompt = config.system_prompt.expect("system prompt present");
        for field in ["task", "projectId", "directoryPrefix", "maxResults"] {
            assert!(prompt.contains(field), "prompt misses input field {field}");
        }
        assert!(prompt.contains("iteration limit"));
    }

    #[test]
    fn workflow_is_dependent_type_for_subgraph_invocation() {
        let template = create_prefetch_workflow();
        assert_eq!(
            template.definition.r#type,
            Some(WorkflowDefinitionType::Dependent)
        );
        assert!(template.definition.variables.is_none());
        assert!(template.definition.available_tools.is_none());
        assert!(template.definition.config.is_none());
        assert!(template.definition.triggered_subworkflow_config.is_none());
    }
}
