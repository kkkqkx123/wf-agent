use anyhow::{Context, Result};

use crate::input::parse_trace;
use crate::model::Trace;

/// Convert an engine snapshot dump into a debuggable trace without taking an
/// engine dependency. Accepted shape (all extra keys ignored):
///
/// ```json
/// {
///   "kind": "workflow",
///   "graph_ref": "my-graph",
///   "steps": [ { "<step-record fields>" } ],
///   "assertions": [],
///   "trigger_templates": [],
///   "budget": { "limit_tokens": 100000 }
/// }
/// ```
///
/// The step list may also sit under the `node_records` key. Each entry is
/// decoded as a step record with defaults filling unrecorded fields.
pub fn import_snapshot(value: &serde_json::Value) -> Result<Trace> {
    let object = value
        .as_object()
        .context("snapshot must be a JSON object")?;
    let kind: crate::model::TraceKind = object
        .get("kind")
        .context("snapshot needs a 'kind' field ('workflow' or 'agent')")
        .and_then(|kind| serde_json::from_value(kind.clone()).context("parse snapshot kind"))?;
    let steps_value = object
        .get("steps")
        .or_else(|| object.get("node_records"))
        .context("snapshot needs a 'steps' (or 'node_records') array")?;
    let steps: Vec<crate::model::StepRecord> =
        serde_json::from_value(steps_value.clone()).context("parse snapshot steps")?;
    let mut trace = Trace {
        schema: crate::model::TRACE_SCHEMA_V1.to_string(),
        kind,
        graph_ref: string_field(object, "graph_ref"),
        agent_template: string_field(object, "agent_template"),
        initial_variables: object
            .get("initial_variables")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        steps,
        assertions: object
            .get("assertions")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        trigger_templates: object
            .get("trigger_templates")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        budget: object
            .get("budget")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
    };
    let text = serde_json::to_string(&trace).context("re-encode imported trace")?;
    trace = parse_trace(&text)?;
    Ok(trace)
}

pub fn import_snapshot_text(text: &str) -> Result<Trace> {
    let value: serde_json::Value = serde_json::from_str(text).context("parse snapshot JSON")?;
    import_snapshot(&value)
}

fn string_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_node_records_shape() {
        let trace = import_snapshot_text(
            r#"{
                "kind": "workflow",
                "graph_ref": "g",
                "node_records": [
                    {"index": 0, "node_id": "a", "success": true}
                ]
            }"#,
        )
        .expect("snapshot imports");
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].node_id, "a");
        assert_eq!(trace.graph_ref, "g");
    }

    #[test]
    fn rejects_missing_kind() {
        let err = import_snapshot_text(r#"{"steps": []}"#).expect_err("kind is required");
        assert!(err.to_string().contains("'kind'"));
    }
}
