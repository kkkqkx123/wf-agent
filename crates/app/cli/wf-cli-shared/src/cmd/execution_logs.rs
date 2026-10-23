//! Execution log queries (single page and follow modes).

use super::execution_stream::{is_terminal_event, is_terminal_status};
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
        // Drain every page before following so a busy execution never leaves
        // tail entries behind when the follow loop moves to `since` queries.
        if let Some(next_cursor) = next {
            cursor = Some(next_cursor);
            continue;
        }
        break;
    }
    // The event stream is the primary follow channel: each bus event pulls
    // fresh entries. The fixed interval survives only as the degraded path
    // when the stream cannot be opened or drops.
    match client
        .get_stream(&wf_client::endpoints::events_stream_for_execution(id, None))
        .await
    {
        Ok(stream) => {
            follow_via_stream(
                cli,
                client,
                id,
                page_size,
                types,
                interval,
                last_seen,
                Box::pin(stream),
            )
            .await
        }
        Err(_) => follow_via_poll(cli, client, id, page_size, types, interval, last_seen).await,
    }
}

/// Stream-driven follow: fresh entries are pulled on bus events (bounded by
/// actual activity, silent when idle) and terminal events end the follow.
/// A dropped stream degrades to interval polling.
#[allow(clippy::too_many_arguments)]
async fn follow_via_stream(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    page_size: usize,
    types: Option<&str>,
    interval: u64,
    mut last_seen: i64,
    mut stream: std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<wf_client::SseFrame, wf_client::ClientError>> + Send>,
    >,
) -> CliResult<()> {
    use futures::StreamExt;
    loop {
        tokio::select! {
            frame = stream.next() => {
                match frame {
                    Some(Ok(frame)) => {
                        let base = wf_client::sse::decode_system_frame(&frame).into_base_event();
                        let Some(base) = base else { continue };
                        if !frame_matches_types(&base, types) {
                            if is_terminal_event(&base.r#type) {
                                break;
                            }
                            continue;
                        }
                        let (fresh, _) = fetch_page(
                            client,
                            id,
                            page_size,
                            types,
                            Some(last_seen.saturating_add(1)),
                            None,
                        )
                        .await?;
                        for entry in &fresh {
                            last_seen = last_seen.max(render_entry(cli, id, entry)?);
                        }
                        if is_terminal_event(&base.r#type) {
                            break;
                        }
                    }
                    Some(Err(_)) | None => {
                        return follow_via_poll(
                            cli, client, id, page_size, types, interval, last_seen,
                        )
                        .await;
                    }
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

/// Interval follow: the degraded path when the event stream is unavailable.
#[allow(clippy::too_many_arguments)]
async fn follow_via_poll(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    page_size: usize,
    types: Option<&str>,
    interval: u64,
    mut last_seen: i64,
) -> CliResult<()> {
    loop {
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

/// Whether a bus event can carry entries of the requested log types.
fn frame_matches_types(base: &wf_types::events::BaseEvent, types: Option<&str>) -> bool {
    let Some(list) = types else {
        return true;
    };
    list.split(',')
        .map(|name| name.trim().to_ascii_uppercase())
        .any(|name| name == base.r#type.as_str())
}

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
