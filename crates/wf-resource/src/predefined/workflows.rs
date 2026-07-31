use serde_json::json;

use wf_types::node::BaseStaticNode;
use wf_types::node::StaticNodeType;
use wf_types::workflow::{
    Edge, EdgeType, TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowMetadata,
    WorkflowTemplate,
};

use crate::registrar::{register_item, Options, Registries};
use crate::result::Summary;

pub const LLM_SUMMARY_WORKFLOW_ID: &str = "llm_summary_workflow";
pub const DEFAULT_LLM_SUMMARY_PROMPT: &str = "Please provide a compressed summary of the following history of the conversation.\n\nRequirements:\n1. retain all significant facts, decisions, and action items\n2. retain requirements or constraints explicitly specified by the user\n3. remove redundant greetings, transition statements and repetitive information\n4. if code snippets exist, retain the description of their function and purpose, and may omit implementation details\n5. limit the length of the summary to 20% of the original length\n\nPlease output the summary directly without any prefixes or explanations.";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn create_llm_summary_workflow(compression_prompt: Option<String>) -> WorkflowTemplate {
    let t = now_ms();

    let nodes = vec![
        BaseStaticNode {
            id: "llm-summary-start".into(),
            node_type: StaticNodeType::StartFromTrigger,
            name: Some("Start LLM Summary".into()),
            description: Some(
                "Receive the full conversation history from the main workflow execution".into(),
            ),
            config: Some(json!({
                "messageInputs": [{
                    "sourceContextId": "conversationHistory",
                    "internalName": "current",
                    "required": true,
                    "description": "Full conversation history to be compressed"
                }]
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "llm-summary-llm".into(),
            node_type: StaticNodeType::Llm,
            name: Some("Summarize Context".into()),
            description: Some(
                "Use LLM to generate a compressed summary of the conversation history".into(),
            ),
            config: Some(json!({
                "profileId": "DEFAULT",
                "contextId": "current",
                "outputContext": "compressed",
                "parameters": {
                    "systemPrompt": compression_prompt.unwrap_or_else(|| DEFAULT_LLM_SUMMARY_PROMPT.into())
                }
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "llm-summary-truncate".into(),
            node_type: StaticNodeType::ContextProcessor,
            name: Some("Replace with Summary".into()),
            description: Some(
                "Replace the original conversation context with the compressed summary only".into(),
            ),
            config: Some(json!({
                "sourceContext": "current",
                "targetContext": "current",
                "operationConfig": {
                    "operation": "TRUNCATE",
                    "strategy": { "type": "KEEP_LAST", "count": 1 },
                    "createNewBatch": true
                },
                "operationOptions": {
                    "visibleOnly": true,
                    "target": "self"
                }
            })),
            execution_config: None,
        },
        BaseStaticNode {
            id: "llm-summary-end".into(),
            node_type: StaticNodeType::ContinueFromTrigger,
            name: Some("Complete LLM Summary".into()),
            description: Some(
                "Pass the compressed conversation summary back to the main workflow execution"
                    .into(),
            ),
            config: Some(json!({
                "messageOutputs": [{
                    "internalName": "current",
                    "targetContextId": "current",
                    "description": "Compressed conversation summary (original context replaced)"
                }]
            })),
            execution_config: None,
        },
    ];

    let edges = vec![
        Edge {
            id: "e-llm-summary-start-to-llm".into(),
            source_node_id: "llm-summary-start".into(),
            target_node_id: "llm-summary-llm".into(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        },
        Edge {
            id: "e-llm-summary-llm-to-truncate".into(),
            source_node_id: "llm-summary-llm".into(),
            target_node_id: "llm-summary-truncate".into(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        },
        Edge {
            id: "e-llm-summary-truncate-to-end".into(),
            source_node_id: "llm-summary-truncate".into(),
            target_node_id: "llm-summary-end".into(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        },
    ];

    WorkflowTemplate {
        id: LLM_SUMMARY_WORKFLOW_ID.into(),
        name: "LLM Summary Workflow".into(),
        description: "LLM-based conversation summarization workflow: summarize -> replace original context with summary".into(),
        definition: WorkflowDefinition {
            id: LLM_SUMMARY_WORKFLOW_ID.into(),
            name: "LLM Summary Workflow".into(),
            description: Some("LLM-based conversation summarization workflow".into()),
            r#type: None,
            version: Some("1.0.0".into()),
            nodes,
            edges,
            config: None,
            variables: None,
            triggers: None,
            triggered_subworkflow_config: Some(TriggeredSubworkflowConfig {
                enable_checkpoints: Some(false),
                timeout: Some(60000),
                max_retries: Some(0),
            }),
            metadata: Some(WorkflowMetadata {
                author: Some("system".into()),
                tags: Some(vec![
                    "context".into(),
                    "compression".into(),
                    "summary".into(),
                    "token".into(),
                    "memory".into(),
                    "predefined".into(),
                ]),
                category: Some("system".into()),
            }),
            available_tools: None,
            created_at: t,
            updated_at: t,
        },
        template_category: Some("system".into()),
        template_tags: Some(vec!["context".into(), "compression".into()]),
        is_public: Some(true),
        enabled: Some(true),
    }
}

pub fn register(regs: &Registries, opts: &Options) -> Summary {
    let wf = create_llm_summary_workflow(None);
    let key = wf.id.clone();
    register_item(&regs.workflows, key, wf, opts.skip_if_exists)
}
