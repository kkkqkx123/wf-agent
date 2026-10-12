//! Execution log queries (single page and follow modes).

use super::execution_stream::is_terminal_status;
use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};
use wf_client::WfClient;

/// Parse a comma-separated event type list; unknown names are rejected so a
/// typo never silently matches nothing.
pub(crate) fn parse_log_types(raw: Option<&str>) -> CliResult<Option<String>> {
    match raw {
        None => Ok(None),
        Some(list) => {
            for s in list.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                serde_json::from_value::<wf_types::events::EventType>(serde_json::Value::String(
                    s.to_string(),
                ))
                .map_err(|_| CliError::Configuration(format!("unknown event type: {s}")))?;
            }
            Ok(Some(list.to_string()))
        }
    }
}

/// Print one page of execution logs, optionally tailing new entries.
pub(crate) async fn show_execution_logs(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    follow: bool,
    limit: Option<usize>,
    types: Option<&str>,
    interval: u64,
) -> CliResult<()> {
    async fn fetch_page(
        client: &WfClient,
        id: &str,
        limit: usize,
        types: Option<&str>,
        since: Option<i64>,
        cursor: Option<&str>,
    ) -> CliResult<(Vec<serde_json::Value>, Option<String>)> {
        let mut params: Vec<String> = Vec::new();
        params.push(format!("limit={limit}"));
        if let Some(cursor) = cursor {
            params.push(format!("cursor={cursor}"));
        }
        if let Some(types) = types {
            params.push(format!("event_types={types}"));
        }
        if let Some(since) = since {
            params.push(format!("since={since}"));
        }
        let page: serde_json::Value = client
            .get_json(&format!(
                "/api/v1/executions/{id}/logs?{}",
                params.join("&")
            ))
            .await
            .map_err(|e| CliError::Configuration(format!("logs query failed: {e}")))?;
        let items = page
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let next = page
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok((items, next))
    }

    fn render_entry(cli: &Cli, id: &str, entry: &serde_json::Value) -> CliResult<i64> {
        let timestamp = entry.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
        if cli.output == OutputFormat::Text {
            let message = entry.get("message").and_then(|v| v.as_str()).unwrap_or("");
            println!("[{timestamp}] {message}");
        } else {
            render_envelope(
                OutputFormat::JsonLines,
                OutputEnvelope::success("execution-log", entry.clone()).with_entity(id.to_string()),
            )?;
        }
        Ok(timestamp)
    }

    let text = cli.output == OutputFormat::Text;
    let page_size = limit.unwrap_or(100);
    let mut cursor: Option<String> = None;
    let mut last_seen = 0i64;
    loop {
        let (items, next) =
            fetch_page(client, id, page_size, types, None, cursor.as_deref()).await?;
        for entry in &items {
            if text || follow {
                last_seen = last_seen.max(render_entry(cli, id, entry)?);
            }
        }
        if !follow {
            if !text {
                let data = serde_json::Value::Array(items);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("execution-logs", data).with_entity(id.to_string()),
                )?;
            }
            return Ok(());
        }
        // Drain every page before polling so a busy execution never leaves
        // tail entries behind when the follow loop moves to `since` polling.
        if let Some(next_cursor) = next {
            cursor = Some(next_cursor);
            continue;
        }
        cursor = None;
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {
                let (fresh, _) =
                    fetch_page(client, id, page_size, types, Some(last_seen.saturating_add(1)), None).await?;
                for entry in &fresh {
                    last_seen = last_seen.max(render_entry(cli, id, entry)?);
                }
                let status: serde_json::Value = client
                    .get_json(&format!("/api/v1/executions/{id}/status"))
                    .await
                    .unwrap_or(serde_json::Value::Null);
                let terminal = status
                    .as_str()
                    .or_else(|| status.get("status").and_then(|s| s.as_str()))
                    .is_some_and(is_terminal_status);
                if terminal {
                    break;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                return Err(CliError::Interrupted("log follow cancelled".to_string()));
            }
        }
    }
    let data = serde_json::json!({"executionId": id, "followed": true});
    render_envelope(
        cli.output,
        OutputEnvelope::success("execution-logs-follow", data).with_entity(id.to_string()),
    )
}
