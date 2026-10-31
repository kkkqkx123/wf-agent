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
        // Client passes raw text through; the server is the single washing
        // point and the single fallback owner for empty agent and model.
        let conversation: Vec<serde_json::Value> = self
            .conversation
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
            .collect();
        wf_client::models::RunAgentLoopBody {
            agent_id: self.agent.clone(),
            model: self.model.clone().unwrap_or_default(),
            message: prompt.to_string(),
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
    /// The server defaults empty ids and empty models to the built-in agent
    /// and model template, so `None` stays empty rather than baking defaults
    /// in here. Iteration budget is intentionally omitted: the server applies its default when
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

/// Parse one `--var KEY=VALUE` assignment. The key must be non-empty;
/// the value parses as JSON when possible and falls back to a plain
/// string so `--var city=paris` stays ergonomic while `--var n=42`
/// keeps its numeric shape.
pub fn parse_workflow_var_arg(arg: &str) -> Result<(String, Value), String> {
    let (key, raw) = arg.split_once('=').ok_or_else(|| {
        format!("invalid --var '{arg}': expected KEY=VALUE")
    })?;
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("invalid --var '{arg}': key must not be empty"));
    }
    let value: Value = serde_json::from_str(raw.trim()).unwrap_or(Value::String(raw.trim().to_string()));
    Ok((key.to_string(), value))
}

/// Parse every `--var` assignment into a top-level map. Duplicate keys
/// resolve last-wins so repeated flags behave like repeated query params.
pub fn parse_workflow_var_args(args: &[String]) -> Result<serde_json::Map<String, Value>, String> {
    let mut map = serde_json::Map::new();
    for arg in args {
        let (key, value) = parse_workflow_var_arg(arg)?;
        map.insert(key, value);
    }
    Ok(map)
}

/// Load workflow input from `--input-file`. Blank files count as no input;
/// non-object JSON is a hard error matching the inline `--input` rules.
pub fn load_workflow_input_file(path: Option<&std::path::Path>) -> Result<Option<Value>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read --input-file '{}': {e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid --input-file JSON: {e}"))?;
    validate_workflow_input_value(Some(&value))?;
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Merge the three workflow input channels with `file < inline < vars`
/// priority at the top level. All three are objects when present; the
/// merged object keeps the shared shape contract (object or empty).
pub fn merge_workflow_inputs(
    file: Option<Value>,
    inline: Option<Value>,
    vars: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let mut merged = serde_json::Map::new();
    for source in [file, inline] {
        if let Some(Value::Object(map)) = source {
            merged.extend(map);
        }
    }
    merged.extend(vars.clone());
    if merged.is_empty() {
        None
    } else {
        Some(Value::Object(merged))
    }
}

/// Resolve the three workflow input channels into one merged value.
/// Parse failures in any channel are hard errors; nothing degrades to
/// empty silently.
pub fn resolve_workflow_input(
    inline: Option<&str>,
    file: Option<&std::path::Path>,
    vars: &[String],
) -> Result<Option<Value>, String> {
    let file_value = load_workflow_input_file(file)?;
    let inline_value = parse_workflow_input(inline)?;
    let var_map = parse_workflow_var_args(vars)?;
    Ok(merge_workflow_inputs(file_value, inline_value, &var_map))
}

/// Local pre-check of merged workflow input against the declared
/// variables fetched from the server. Mirrors the server gate
/// (backfill defaults, then type plus required) and reports every issue
/// together so one round fixes all keys.
pub fn prevalidate_workflow_input_locally(
    input: Option<&Value>,
    variables: Option<&[wf_types::workflow_execution::VariableDefinition]>,
) -> Result<(), String> {
    prevalidate_workflow_input_locally_with_policy(input, variables, false)
}

/// Policy variant of [`prevalidate_workflow_input_locally`]. `strict`
/// rejects unknown top-level keys; the definition side owns the switch.
pub fn prevalidate_workflow_input_locally_with_policy(
    input: Option<&Value>,
    variables: Option<&[wf_types::workflow_execution::VariableDefinition]>,
    strict: bool,
) -> Result<(), String> {
    let issues = wf_types::workflow_execution::collect_workflow_input_issues_with_policy(
        input, variables, strict,
    );
    if issues.is_empty() {
        return Ok(());
    }
    Err(format!(
        "invalid workflow input ({} error(s)): {}",
        issues.len(),
        issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    ))
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
    fn agent_body_passes_raw_text_for_server_wash() {
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
        assert_eq!(body["message"], "\x1b[31mhi\x1b[0m");
        assert_eq!(body["model"], "");
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
    fn workflow_var_args_parse_and_merge_with_priority() {
        let vars = parse_workflow_var_args(&[
            "city=paris".to_string(),
            "count=42".to_string(),
            "raw=hello world".to_string(),
        ])
        .expect("vars parse");
        assert_eq!(vars["city"], serde_json::json!("paris"));
        assert_eq!(vars["count"], serde_json::json!(42));
        let merged = merge_workflow_inputs(
            Some(serde_json::json!({"city": "london", "other": 1})),
            Some(serde_json::json!({"city": "berlin"})),
            &vars,
        )
        .expect("merged");
        assert_eq!(merged["city"], serde_json::json!("paris"));
        assert_eq!(merged["count"], serde_json::json!(42));
        assert_eq!(merged["other"], serde_json::json!(1));
        assert!(parse_workflow_var_arg("novalue").is_err());
        assert!(parse_workflow_var_arg("=x").is_err());
    }

    #[test]
    fn workflow_input_file_and_resolve_merge() {
        let dir = std::env::temp_dir();
        let file = dir.join(format!("wf-input-{}.json", std::process::id()));
        std::fs::write(&file, r#"{"a": 1, "b": 2}"#).expect("fixture");
        let loaded = load_workflow_input_file(Some(&file)).expect("file loads").expect("object");
        assert_eq!(loaded["a"], 1);
        let merged = resolve_workflow_input(
            Some(r#"{"b": 3}"#),
            Some(&file),
            &["c=4".to_string()],
        )
        .expect("resolve")
        .expect("merged");
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"], 3);
        assert_eq!(merged["c"], 4);
        let _ = std::fs::remove_file(&file);
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
