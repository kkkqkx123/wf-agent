use wf_api::infra::events::{self, EventQueryOptions};
use wf_types::events::EventType;

use crate::args::{Cli, EventSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &EventSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        EventSub::List {
            execution,
            types,
            limit,
        } => {
            let event_types = types.as_deref().map(|s| {
                s.split(',')
                    .filter_map(|v| parse_event_type(v.trim()))
                    .collect::<Vec<_>>()
            });
            let opts = EventQueryOptions {
                execution_id: execution.clone(),
                agent_loop_id: None,
                workflow_id: None,
                event_types,
                limit: *limit,
            };
            let events = events::history(ctx, &opts).await?;
            let data = serde_json::to_value(&events)?;
            render_envelope(cli.output, OutputEnvelope::success("event-list", data))
        }
        EventSub::Stats => {
            let stats = events::stats(ctx).await?;
            let data = serde_json::to_value(&stats)?;
            render_envelope(cli.output, OutputEnvelope::success("event-stats", data))
        }
        EventSub::Timeline { id } => {
            let timeline = events::timeline(ctx, id).await?;
            let data = serde_json::to_value(&timeline)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("event-timeline", data).with_entity(id.clone()),
            )
        }
        EventSub::Follow { id } => {
            let timeline = events::timeline(ctx, id).await?;
            let data = serde_json::to_value(&timeline)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("event-follow", data).with_entity(id.clone()),
            )
        }
    };
    adapter.shutdown().await?;
    result
}

fn parse_event_type(s: &str) -> Option<EventType> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .ok()
        .or_else(|| serde_json::from_str(&format!("\"{s}\"")).ok())
}
