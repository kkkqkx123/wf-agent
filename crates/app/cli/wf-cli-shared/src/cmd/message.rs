use crate::args::{Cli, MessageSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &MessageSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        MessageSub::List {
            execution,
            role: _,
            limit,
        } => {
            let limit = limit.unwrap_or(100);
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/messages/by-execution/{execution}?limit={limit}&offset=0"
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("message-list", data).with_entity(execution.clone()),
            )
        }
        MessageSub::Search { query, limit } => {
            let mut path = format!("/api/v1/messages/search?q={query}");
            if let Some(l) = limit {
                path.push_str(&format!("&limit={l}"));
            }
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(cli.output, OutputEnvelope::success("message-search", data))
        }
    }
}
