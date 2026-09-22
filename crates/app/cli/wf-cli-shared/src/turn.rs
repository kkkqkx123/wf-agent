use std::sync::Arc;

use serde_json::Value;
use wf_api::infra::context::ApiContext;
use wf_api::infra::error::ApiError;
use wf_api::infra::stream::ExecutionEventStream;
use wf_api::AgentLoopInput;
use wf_types::Id;

use wf_api::build_agent_loop_config;

/// What a turn executes.
#[derive(Debug, Clone)]
pub enum TurnKind {
    Agent {
        prompt: String,
    },
    Workflow {
        workflow_id: String,
        input: Option<Value>,
    },
}

/// Parameters for a turn, shared between headless and mini forms.
#[derive(Debug, Clone)]
pub struct TurnParams {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub approve_prefixes: Vec<String>,
    /// Prior conversation seeding the run. Headless and fullscreen forms
    /// leave this empty (single-shot turns); the mini session fills it from
    /// its transcript.
    pub conversation: Vec<wf_types::message::Message>,
    pub kind: TurnKind,
}

impl TurnParams {
    pub fn prompt(&self) -> Option<&str> {
        match &self.kind {
            TurnKind::Agent { prompt } => Some(prompt.as_str()),
            TurnKind::Workflow { .. } => None,
        }
    }
}

/// Build agent loop params from turn params with approval wiring. The
/// engine evaluates `approval_options` first (denials are terminal) and
/// routes only `Ask` decisions to `approval_handler`. Centralizes the
/// config assembly so future fields only change here.
pub fn build_agent_loop_params(
    params: &TurnParams,
    approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    approval_handler: Option<Arc<dyn wf_api::ToolApprovalHandler>>,
) -> wf_api::agent::agent_execution::RunAgentLoopParams {
    let prompt = match &params.kind {
        TurnKind::Agent { prompt } => prompt.clone(),
        TurnKind::Workflow { .. } => String::new(),
    };
    let sanitized = crate::sanitize::sanitize_user_text(&prompt);
    wf_api::agent::agent_execution::RunAgentLoopParams {
        agent_loop_id: Some(Id::from(wf_common::generate_id())),
        approval_options,
        approval_handler,
        config: build_agent_loop_config(params.agent.clone(), params.model.clone()),
        input: AgentLoopInput {
            message: sanitized,
            context: std::collections::HashMap::new(),
            conversation: params.conversation.clone(),
        },
    }
}

/// Stream an agent turn. Returns the execution id and the event stream.
pub async fn stream_agent_turn(
    ctx: &ApiContext,
    params: &TurnParams,
    approval_options: Option<wf_types::tool::approval::ToolApprovalOptions>,
    approval_handler: Option<Arc<dyn wf_api::ToolApprovalHandler>>,
) -> Result<(String, ExecutionEventStream), ApiError> {
    // Composition boundary: resolve the agent template (built-in
    // `@standard/main` default, user overrides first) before execution. The
    // shared prompt module renders header, tail and exposure from full context.
    let env = wf_execution_shared::agent_prompt::PromptEnvironment::new(
        Some(ctx.registries.as_ref()),
        Some(ctx.tool_registry.as_ref()),
        ctx.metrics.as_deref(),
    );
    let run_params = wf_api::composition::agent::resolve_run_params(
        &env,
        build_agent_loop_params(params, approval_options, approval_handler),
    )?;
    let execution_id = run_params
        .agent_loop_id
        .as_ref()
        .map(|id| id.to_string())
        .unwrap_or_else(wf_common::generate_id);
    let stream = wf_api::agent::agent_execution::stream(ctx, run_params).await?;
    Ok((execution_id, stream))
}

/// Stream a workflow turn. Returns the execution id and the event stream.
pub async fn stream_workflow_turn(
    ctx: Arc<ApiContext>,
    workflow_id: &str,
    input: Option<Value>,
) -> Result<(String, ExecutionEventStream), ApiError> {
    let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
        workflow_id: workflow_id.to_string(),
        input,
        options: None,
    };
    let (id, stream) = wf_api::workflow::workflow_execution::stream(ctx, params).await?;
    Ok((id.to_string(), stream))
}

/// Helper to parse workflow input JSON string.
pub fn parse_workflow_input(input: Option<&str>) -> Result<Option<Value>, String> {
    match input {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => serde_json::from_str(s)
            .map(Some)
            .map_err(|e| format!("invalid --input JSON: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_workflow_input_empty_is_none() {
        assert_eq!(parse_workflow_input(None).unwrap(), None);
        assert_eq!(parse_workflow_input(Some("")).unwrap(), None);
        assert_eq!(parse_workflow_input(Some("  ")).unwrap(), None);
    }

    #[test]
    fn parse_workflow_input_valid_json() {
        let v = parse_workflow_input(Some(r#"{"a":1}"#)).unwrap().unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn parse_workflow_input_invalid_json_errors() {
        assert!(parse_workflow_input(Some("{bad")).is_err());
    }

    #[test]
    fn build_agent_loop_params_uses_defaults() {
        let params = TurnParams {
            agent: None,
            model: None,
            approve_prefixes: vec![],
            conversation: Vec::new(),
            kind: TurnKind::Agent {
                prompt: "hi".to_string(),
            },
        };
        let run = build_agent_loop_params(&params, None, None);
        assert_eq!(run.config.model, wf_api::DEFAULT_MODEL);
        assert_eq!(run.input.message, "hi");
        assert!(run.input.conversation.is_empty());
    }

    #[test]
    fn build_agent_loop_params_carries_conversation() {
        let history = vec![wf_types::message::Message {
            id: wf_types::Id::from("m1".to_string()),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("earlier".to_string()),
            timestamp: 1,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }];
        let params = TurnParams {
            agent: None,
            model: None,
            approve_prefixes: vec![],
            conversation: history,
            kind: TurnKind::Agent {
                prompt: "follow-up".to_string(),
            },
        };
        let run = build_agent_loop_params(&params, None, None);
        assert_eq!(run.input.conversation.len(), 1);
        assert_eq!(run.input.message, "follow-up");
    }

    #[test]
    fn build_agent_loop_params_sanitizes_input() {
        let params = TurnParams {
            agent: Some("ag".into()),
            model: Some("m".into()),
            approve_prefixes: vec![],
            conversation: Vec::new(),
            kind: TurnKind::Agent {
                prompt: "\x1b[31mhi\x1b[0m".to_string(),
            },
        };
        let run = build_agent_loop_params(&params, None, None);
        assert_eq!(run.input.message, "hi");
        assert_eq!(run.config.agent_id.to_string(), "ag");
        assert_eq!(run.config.model, "m");
    }
}
