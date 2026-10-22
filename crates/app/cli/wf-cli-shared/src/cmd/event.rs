use futures::StreamExt;

use crate::args::{Cli, EventSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;
use wf_client::WfClient;

pub async fn run(cli: &Cli, sub: &EventSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        EventSub::List {
            execution,
            workflow,
            agent_loop,
            types,
            limit,
        } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(id) = execution {
                params.push(format!("execution_id={id}"));
            }
            if let Some(id) = workflow {
                params.push(format!("workflow_id={id}"));
            }
            if let Some(id) = agent_loop {
                params.push(format!("agent_loop_id={id}"));
            }
            if let Some(l) = limit {
                params.push(format!("limit={l}"));
            }
            let path = if params.is_empty() {
                "/api/v1/events".to_string()
            } else {
                format!("/api/v1/events?{}", params.join("&"))
            };
            let mut data: serde_json::Value = client.get_json(&path).await?;
            if let Some(filter) = types {
                let wanted: Vec<String> = filter.split(',').map(|v| v.trim().to_string()).collect();
                if let Some(items) = data.get_mut("items").and_then(|v| v.as_array_mut()) {
                    items.retain(|item| {
                        item.get("type")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| wanted.iter().any(|w| w == t))
                    });
                }
            }
            render_envelope(cli.output, OutputEnvelope::success("event-list", data))
        }
        EventSub::Stats => {
            let data: serde_json::Value = client.get_json("/api/v1/events/stats").await?;
            render_envelope(cli.output, OutputEnvelope::success("event-stats", data))
        }
        EventSub::Timeline { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/events/timeline/{id}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("event-timeline", data).with_entity(id.clone()),
            )
        }
        EventSub::Follow {
            id,
            types: _,
            workflow,
            once,
        } => {
            if *once {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/events/timeline/{id}"))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("event-follow", data).with_entity(id.clone()),
                )
            } else {
                follow_stream(cli, client, id, workflow.as_deref()).await
            }
        }
    }
}

/// Stream live events over SSE, printing history first like the follower
/// did, then live deltas until Ctrl-C or a terminal event.
async fn follow_stream(
    cli: &Cli,
    client: &WfClient,
    id: &str,
    workflow: Option<&str>,
) -> CliResult<()> {
    let history: serde_json::Value = client
        .get_json(&format!("/api/v1/events/timeline/{id}"))
        .await
        .unwrap_or(serde_json::Value::Null);
    let history_items = history
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| history.as_array().cloned())
        .unwrap_or_default();
    if cli.output == crate::output::OutputFormat::Text {
        for ev in &history_items {
            if let Ok(base) =
                serde_json::from_value::<wf_types::events::BaseEvent>(ev.clone())
            {
                println!(
                    "[{}] {} {}",
                    base.timestamp,
                    base.r#type.as_str(),
                    base.execution_id.as_deref().unwrap_or("-")
                );
            }
        }
    } else if !history_items.is_empty() {
        render_envelope(
            cli.output,
            OutputEnvelope::success("event-follow-history", history.clone())
                .with_entity(id.to_string()),
        )?;
    }

    let path = wf_client::endpoints::events_stream_for_execution(id, workflow);
    let mut stream = client.get_stream(&path).await?;
    let mut live_count = 0usize;
    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(frame)) => {
                        match wf_client::sse::decode_system_frame(&frame) {
                            wf_client::sse::SystemFrame::Connected(_) => continue,
                            wf_client::sse::SystemFrame::Raw(_) => continue,
                            wf_client::sse::SystemFrame::Base(base) => {
                                live_count += 1;
                                if cli.output == crate::output::OutputFormat::Text {
                                    println!(
                                        "[{}] {} {}",
                                        base.timestamp,
                                        base.r#type.as_str(),
                                        base.execution_id.as_deref().unwrap_or("-")
                                    );
                                } else {
                                    let data = serde_json::to_value(&base)
                                        .unwrap_or(serde_json::Value::Null);
                                    render_envelope(
                                        crate::output::OutputFormat::JsonLines,
                                        OutputEnvelope::success("event-follow", data)
                                            .with_entity(id.to_string()),
                                    )?;
                                }
                                if is_terminal_event(&base.r#type) {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Err(e)) => return Err(crate::error::CliError::from(e)),
                    None => break,
                }
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    let data = serde_json::json!({"executionId": id, "followed": true, "liveEvents": live_count});
    render_envelope(
        cli.output,
        OutputEnvelope::success("event-follow", data).with_entity(id.to_string()),
    )
}

fn is_terminal_event(event_type: &wf_types::events::EventType) -> bool {
    let name = event_type.as_str();
    name.ends_with("COMPLETED") || name.ends_with("FAILED") || name.ends_with("CANCELLED")
}
