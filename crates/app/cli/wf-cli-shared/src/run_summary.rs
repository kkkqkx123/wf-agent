//! End-of-session summary envelopes and duration formatting.

use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat, OutputSink};
use crate::run::RunOptions;
use serde_json::Value;
use wf_api::{DEFAULT_AGENT, DEFAULT_MODEL};

pub(crate) fn had_output_for_result(result: &Value) -> bool {
    !result.is_null()
        && result
            .as_str()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(true)
}

/// Render a workflow result in text mode; structured formats already carry
/// the value inside the closing summary envelope.
pub(crate) fn render_value_text(sink: &mut dyn OutputSink, result: &Value) -> CliResult<()> {
    let text = match result {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    if !text.is_empty() {
        sink.write_chunk(&text)?;
        if !text.ends_with('\n') {
            sink.write_chunk("\n")?;
        }
    }
    Ok(())
}

/// End-of-session summary (text) or terminal envelope (json/jsonl).
/// Parameters for the summary line / envelope.
pub(crate) struct SummaryParams<'a> {
    pub(crate) sink: &'a mut dyn OutputSink,
    pub(crate) format: OutputFormat,
    pub(crate) execution_id: &'a str,
    pub(crate) iterations: u32,
    pub(crate) duration_ms: u64,
    pub(crate) had_output: bool,
    pub(crate) opts: &'a RunOptions,
    pub(crate) result: Option<&'a serde_json::Value>,
}

pub(crate) fn write_summary(p: SummaryParams<'_>) -> CliResult<()> {
    let duration = format_duration(p.duration_ms);
    match p.format {
        OutputFormat::Text => {
            let line = if p.had_output {
                format!(
                    "▣ {} · {} iterations · {duration}",
                    p.execution_id, p.iterations
                )
            } else {
                format!("▣ {} · no output · {duration}", p.execution_id)
            };
            p.sink.write_raw(&line)
        }
        OutputFormat::Json => {
            let mut data = serde_json::json!({
                "executionId": p.execution_id,
                "iterations": p.iterations,
                "durationMs": p.duration_ms,
                "hadOutput": p.had_output,
                "model": p.opts.model.clone().unwrap_or_else(|| DEFAULT_MODEL.to_string()),
                "agentId": p.opts.agent_id.clone().unwrap_or_else(|| DEFAULT_AGENT.to_string()),
            });
            if let Some(res) = p.result {
                data.as_object_mut()
                    .expect("json object")
                    .insert("result".to_string(), res.clone());
            }
            let envelope = OutputEnvelope::success("execution", data).with_entity("agent-loop");
            if let Some(line) = envelope.render(p.format) {
                p.sink.write_raw(&line)
            } else {
                Ok(())
            }
        }
        OutputFormat::JsonLines => {
            let mut record = serde_json::json!({
                "type": "execution_summary",
                "executionId": p.execution_id,
                "iterations": p.iterations,
                "durationMs": p.duration_ms,
                "hadOutput": p.had_output,
                "success": true,
            });
            if let Some(res) = p.result {
                record
                    .as_object_mut()
                    .expect("json object")
                    .insert("result".to_string(), res.clone());
            }
            let line = serde_json::to_string(&record)?;
            p.sink.write_raw(&line)
        }
        OutputFormat::Silent => Ok(()),
    }
    .map_err(CliError::from)
}

/// Failure envelope for structured formats (text diagnostics go through
/// stderr in `main`).
pub(crate) fn write_failure_envelope(
    sink: &mut dyn OutputSink,
    format: OutputFormat,
    execution_id: &str,
    error: &str,
) {
    let write = |record: String| sink.write_raw(&record).map_err(CliError::from);
    let result = match format {
        OutputFormat::Json => OutputEnvelope::failure("execution", error)
            .with_entity("agent-loop")
            .render(format)
            .map(write),
        OutputFormat::JsonLines => Some(
            serde_json::to_string(&serde_json::json!({
                "type": "execution_summary",
                "executionId": execution_id,
                "success": false,
                "error": error,
            }))
            .map_err(CliError::from)
            .and_then(write),
        ),
        _ => None,
    };
    if let Some(Err(err)) = result {
        tracing::warn!(target: "wf_cli", error = %err, "failed to write failure envelope");
    }
}

/// Human duration: `123ms` under a second, `1.3s` above.
pub(crate) fn format_duration(duration_ms: u64) -> String {
    if duration_ms < 1_000 {
        format!("{duration_ms}ms")
    } else {
        format!("{:.1}s", duration_ms as f64 / 1_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MemorySink;

    #[test]
    fn summary_line_reflects_output_presence_and_duration_format() {
        assert_eq!(format_duration(950), "950ms");
        assert_eq!(format_duration(1_234), "1.2s");

        let mut sink = MemorySink::new();
        write_summary(SummaryParams {
            sink: &mut sink,
            format: OutputFormat::Text,
            execution_id: "exec-1",
            iterations: 3,
            duration_ms: 1_234,
            had_output: true,
            opts: &RunOptions::default(),
            result: None,
        })
        .unwrap();
        assert_eq!(sink.raw(), vec!["▣ exec-1 · 3 iterations · 1.2s"]);

        let mut sink = MemorySink::new();
        write_summary(SummaryParams {
            sink: &mut sink,
            format: OutputFormat::Text,
            execution_id: "exec-2",
            iterations: 0,
            duration_ms: 5,
            had_output: false,
            opts: &RunOptions::default(),
            result: None,
        })
        .unwrap();
        assert_eq!(sink.raw(), vec!["▣ exec-2 · no output · 5ms"]);
    }

    #[test]
    fn json_summary_envelope_carries_execution_fields() {
        let mut sink = MemorySink::new();
        write_summary(SummaryParams {
            sink: &mut sink,
            format: OutputFormat::Json,
            execution_id: "exec-3",
            iterations: 2,
            duration_ms: 42,
            had_output: true,
            opts: &RunOptions {
                model: Some("mock".into()),
                ..Default::default()
            },
            result: None,
        })
        .unwrap();
        let raw = sink.raw()[0];
        let parsed: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["type"], "execution");
        assert_eq!(parsed["entity"], "agent-loop");
        assert_eq!(parsed["data"]["executionId"], "exec-3");
        assert_eq!(parsed["data"]["iterations"], 2);
        assert_eq!(parsed["data"]["durationMs"], 42);
        assert_eq!(parsed["data"]["model"], "mock");
        assert!(parsed["timestamp"].as_i64().unwrap() > 0);
    }
}
