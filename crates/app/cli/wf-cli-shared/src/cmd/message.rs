use wf_api::entity::message;

use crate::args::{Cli, MessageSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &MessageSub) -> CliResult<()> {
    let domain =
        crate::domain::DomainHandle::require_embedded(cli, crate::mode::CliMode::Run, "message")
            .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let result = match sub {
        MessageSub::List {
            execution,
            role: _,
            limit,
        } => {
            let msgs = message::by_execution_paginated(
                ctx,
                execution,
                0,
                limit.unwrap_or(100),
                message::MessageOrder::Asc,
            )
            .await?;
            let data = serde_json::to_value(&msgs)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("message-list", data).with_entity(execution.clone()),
            )
        }
        MessageSub::Search { query, limit } => {
            let msgs = message::search(ctx, query, *limit).await?;
            let data = serde_json::to_value(&msgs)?;
            render_envelope(cli.output, OutputEnvelope::success("message-search", data))
        }
    };
    domain.shutdown().await?;
    result
}
