//! Turn parameters and server request bodies shared by the CLI forms.
//!
//! A turn executes either an agent message or a workflow. This module owns
//! the UI-facing shape plus its translation into the `wf-server` wire bodies;
//! (`RunAgentLoopBody` / `ExecuteBody`); the server re-runs template
//! resolution and defaults, so the client only fills what the flags provide.

use serde_json::Value;

/// Built-in agent definition id used when no `--agent` is given. Mirrors the
/// server default for empty ids.
pub const DEFAULT_AGENT: &str = "@standard/main";

/// Model profile id used when no `--model` is given.
pub const DEFAULT_MODEL: &str = "default";

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
    /// Server-side run budget in milliseconds (`--timeout`).
    pub timeout_ms: Option<u64>,
    /// Structured agent context (`--context` / `--context-file`).
    /// The server validates it against the agent `context_schema` when the
    /// template declares one; empty maps skip that gate.
    pub context: serde_json::Map<String, Value>,
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

    /// Translate into the typed `POST /api/v1/agent-loops/{id}/run|stream` body.
    /// Single source for both the SSE stream (serialized to JSON) and the
    /// blocking fallback (passed to the generated client directly), so field
    /// names cannot drift between the two paths.
    pub fn agent_typed_body(&self) -> wf_client::models::RunAgentLoopBody {
        let prompt = self.prompt().unwrap_or_default();
        let sanitized = crate::sanitize::sanitize_user_text(prompt);
        let conversation: Vec<serde_json::Value> = self
            .conversation
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
            .collect();
        wf_client::models::RunAgentLoopBody {
            agent_id: self.agent.clone(),
            model: self
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            message: sanitized,
            context: self.context.clone(),
            conversation: Some(conversation),
            max_execution_time: self.timeout_ms.map(|v| v as i64),
            max_iterations: None,
            tool_call_protocol: Value::Null,
            available_tool_names: None,
            checkpoint_message_interval: None,
            discoverable_tool_names: None,
            enable_general_tool: None,
            enable_token_tracking: None,
            file_scope: None,
            hidden_tool_names: None,
            hooks: None,
            initial_tool_names: None,
            token_limit: None,
            token_warning_threshold: None,
        }
    }

    /// Translate into the `POST /api/v1/agent-loops/{id}/run|stream` body.
    /// The server defaults empty ids to the built-in agent, so `None` stays
    /// empty rather than baking the default in here. `model` uses the shared
    /// sentinel because the wire body requires the field; the server treats
    /// the sentinel the same as empty and lets the template win. Iteration
    /// budget is intentionally omitted: the server applies its default when
    /// neither the request nor the template sets one, so a hardcoded value
    /// here would override template configuration.
    pub fn agent_request_body(&self) -> Value {
        serde_json::to_value(self.agent_typed_body()).unwrap_or(Value::Null)
    }

    /// Typed `POST /api/v1/workflows/{id}/execute*` body.
    pub fn workflow_typed_body(input: Option<&Value>) -> wf_client::models::ExecuteBody {
        wf_client::models::ExecuteBody {
            input: input.cloned(),
            file_scope: None,
            version: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: None,
            node_timeout: None,
            max_pause_duration: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
            debug: None,
        }
    }

    /// Translate into the `POST /api/v1/workflows/{id}/execute*` body.
    pub fn workflow_request_body(input: Option<&Value>) -> Value {
        serde_json::to_value(Self::workflow_typed_body(input)).unwrap_or(Value::Null)
    }
}

/// Validate an already-parsed workflow input value against the shared
/// shape rules (W1-W3): null/missing is no input, an object passes
/// through, anything else (array, string, number, bool) is a hard error.
pub fn validate_workflow_input_value(input: Option<&Value>) -> Result<(), String> {
    wf_types::workflow_execution::validate_workflow_input_shape(input)
        .map_err(|reason| format!("invalid --input: {reason}"))
}

fn input_kind(value: &Value) -> &'static str {
    if value.is_array() {
        "array"
    } else if value.is_string() {
        "string"
    } else if value.is_number() {
        "number"
    } else {
        "boolean"
    }
}

/// Parse the structured agent context from `--context` (inline JSON
/// object) and `--context-file` (JSON file holding an object). File
/// content loads first so inline keys win on conflict; both must be JSON
/// objects when present. Failures are hard errors: context silently
/// degrading to empty would hide a typo from the user.
pub fn parse_agent_context(
    inline: Option<&str>,
    file: Option<&std::path::Path>,
) -> Result<serde_json::Map<String, Value>, String> {
    let mut merged = serde_json::Map::new();
    if let Some(path) = file {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read --context-file '{}': {e}", path.display()))?;
        match serde_json::from_str::<Value>(&text)
            .map_err(|e| format!("invalid --context-file JSON: {e}"))?
        {
            Value::Object(map) => merged.extend(map),
            other => {
                return Err(format!(
                    "invalid --context-file: must be a JSON object, got {}",
                    input_kind(&other),
                ));
            }
        }
    }
    match inline {
        None => Ok(merged),
        Some(s) if s.trim().is_empty() => Ok(merged),
        Some(s) => {
            match serde_json::from_str::<Value>(s)
                .map_err(|e| format!("invalid --context JSON: {e}"))?
            {
                Value::Object(map) => {
                    merged.extend(map);
                    Ok(merged)
                }
                Value::Null => Ok(merged),
                other => Err(format!(
                    "invalid --context: must be a JSON object, got {}",
                    input_kind(&other),
                )),
            }
        }
    }
}

/// Helper to parse workflow input JSON string.
pub fn parse_workflow_input(input: Option<&str>) -> Result<Option<Value>, String> {
    match input {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => {
            let value: Value =
                serde_json::from_str(s).map_err(|e| format!("invalid --input JSON: {e}"))?;
            validate_workflow_input_value(Some(&value))?;
            if value.is_null() {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        }
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
    fn parse_workflow_input_rejects_non_objects() {
        assert!(parse_workflow_input(Some(r#"[1,2]"#)).is_err());
        assert!(parse_workflow_input(Some(r#""text""#)).is_err());
        assert!(parse_workflow_input(Some("42")).is_err());
        assert!(parse_workflow_input(Some("true")).is_err());
        assert_eq!(
            parse_workflow_input(Some("{}")).unwrap().unwrap(),
            Value::Object(Default::default())
        );
        assert_eq!(parse_workflow_input(Some("null")).unwrap(), None);
    }

    #[test]
    fn validate_workflow_input_value_gates_shapes() {
        assert!(validate_workflow_input_value(None).is_ok());
        assert!(validate_workflow_input_value(Some(&Value::Null)).is_ok());
        assert!(validate_workflow_input_value(Some(&serde_json::json!({"a": 1}))).is_ok());
        assert!(validate_workflow_input_value(Some(&serde_json::json!([1]))).is_err());
        assert!(validate_workflow_input_value(Some(&serde_json::json!("x"))).is_err());
    }

    #[test]
    fn agent_body_carries_prompt_and_conversation() {
        let params = TurnParams {
            agent: Some("ag".into()),
            model: Some("m".into()),
            approve_prefixes: vec![],
            timeout_ms: Some(5000),
            context: Default::default(),
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
        assert!(body.get("max_iterations").is_none());
    }

    #[test]
    fn agent_body_sanitizes_input() {
        let params = TurnParams {
            agent: None,
            model: None,
            approve_prefixes: vec![],
            timeout_ms: None,
            context: Default::default(),
            conversation: Vec::new(),
            kind: TurnKind::Agent {
                prompt: "\x1b[31mhi\x1b[0m".to_string(),
            },
        };
        let body = params.agent_request_body();
        assert_eq!(body["message"], "hi");
        assert_eq!(body["model"], DEFAULT_MODEL);
    }

    #[test]
    fn agent_body_carries_context() {
        let params = TurnParams {
            agent: None,
            model: None,
            approve_prefixes: vec![],
            timeout_ms: None,
            context: parse_agent_context(Some(r#"{"city": "paris"}"#), None)
                .expect("context parses"),
            conversation: Vec::new(),
            kind: TurnKind::Agent {
                prompt: "hi".to_string(),
            },
        };
        let body = params.agent_request_body();
        assert_eq!(body["context"]["city"], "paris");
    }

    #[test]
    fn agent_context_rejects_non_objects_and_merges_file() {
        assert!(parse_agent_context(Some("[1]"), None).is_err());
        assert!(parse_agent_context(Some("42"), None).is_err());
        assert!(parse_agent_context(None, None).expect("empty").is_empty());
        assert!(
            parse_agent_context(Some("  "), None)
                .expect("blank")
                .is_empty()
        );
        let dir = std::env::temp_dir();
        let file = dir.join(format!("wf-context-{}.json", std::process::id()));
        std::fs::write(&file, r#"{"a": 1, "b": 2}"#).expect("fixture");
        let merged = parse_agent_context(Some(r#"{"b": 3}"#), Some(&file))
            .expect("file plus inline");
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"], 3);
        let _ = std::fs::remove_file(&file);
        assert!(parse_agent_context(None, Some(&file)).is_err());
    }
}
