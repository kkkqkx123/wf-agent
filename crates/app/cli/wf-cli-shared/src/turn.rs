//! Turn parameters and server request bodies shared by the CLI forms.
//!
//! A turn executes either an agent message or a workflow. This module owns
//! the UI-facing shape plus its translation into the `wf-server` wire bodies;
//! (`RunAgentLoopBody` / `ExecuteBody`); the server re-runs template
//! resolution and defaults, so the client only fills what the flags provide.

use serde_json::Value;

/// What a turn executes.
#[derive(Debug, Clone)]
pub enum TurnKind {
    Agent { prompt: String },
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
    /// Server-side run budget in milliseconds (`--timeout`).
    pub timeout_ms: Option<u64>,
    /// Prior conversation seeding the run. Headless single-shot turns leave
    /// this empty; the mini session fills it from its transcript.
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

    /// Translate into the `POST /api/v1/agent-loops/{id}/run|stream` body.
    /// The server defaults empty ids to the built-in agent, so `None` stays
    /// empty rather than baking the default in here.
    pub fn agent_request_body(&self) -> Value {
        let prompt = self.prompt().unwrap_or_default();
        let sanitized = crate::sanitize::sanitize_user_text(prompt);
        let conversation =
            serde_json::to_value(&self.conversation).unwrap_or(Value::Null);
        let mut body = serde_json::json!({
            "agent_id": self.agent.clone().unwrap_or_default(),
            "model": self.model.clone().unwrap_or_else(|| wf_client::DEFAULT_MODEL.to_string()),
            "message": sanitized,
            "max_iterations": 50,
            "context": {},
            "conversation": conversation,
        });
        if let Some(timeout) = self.timeout_ms {
            body["max_execution_time"] = timeout.into();
        }
        body
    }

    /// Translate into the `POST /api/v1/workflows/{id}/execute*` body.
    pub fn workflow_request_body(input: Option<&Value>) -> Value {
        serde_json::json!({ "input": input })
    }
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
    fn agent_body_carries_prompt_and_conversation() {
        let params = TurnParams {
            agent: Some("ag".into()),
            model: Some("m".into()),
            approve_prefixes: vec![],
            timeout_ms: Some(5000),
            conversation: Vec::new(),
            kind: TurnKind::Agent {
                prompt: "hi".to_string(),
            },
        };
        let body = params.agent_request_body();
        assert_eq!(body["agent_id"], "ag");
        assert_eq!(body["model"], "m");
        assert_eq!(body["message"], "hi");
        assert_eq!(body["max_execution_time"], 5000);
    }

    #[test]
    fn agent_body_sanitizes_input() {
        let params = TurnParams {
            agent: None,
            model: None,
            approve_prefixes: vec![],
            timeout_ms: None,
            conversation: Vec::new(),
            kind: TurnKind::Agent {
                prompt: "\x1b[31mhi\x1b[0m".to_string(),
            },
        };
        let body = params.agent_request_body();
        assert_eq!(body["message"], "hi");
        assert_eq!(body["model"], wf_client::DEFAULT_MODEL);
    }
}
