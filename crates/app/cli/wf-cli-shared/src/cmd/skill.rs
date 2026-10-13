//! Retained passthrough: skill records are schemaless user content.

use crate::args::{Cli, SkillSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &SkillSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        SkillSub::List => {
            let data: serde_json::Value = client.get_json("/api/v1/skills").await?;
            render_envelope(cli.output, OutputEnvelope::success("skill-list", data))
        }
        SkillSub::Query { filter } => {
            let path = match filter {
                Some(q) => format!("/api/v1/skills/query?filter={q}"),
                None => "/api/v1/skills/query".to_string(),
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(cli.output, OutputEnvelope::success("skill-query", data))
        }
        SkillSub::Show { name } => {
            let data: serde_json::Value =
                client.get_json(&format!("/api/v1/skills/{name}")).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-show", data).with_entity(name.clone()),
            )
        }
        SkillSub::Enable { name } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/skills/{name}/enable"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"enabled": name});
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-enable", data).with_entity(name.clone()),
            )
        }
        SkillSub::Disable { name } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/skills/{name}/disable"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"disabled": name});
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-disable", data).with_entity(name.clone()),
            )
        }
        SkillSub::Scan => {
            let data: serde_json::Value = client
                .post_json("/api/v1/skills/scan", &serde_json::json!({}))
                .await?;
            let count = data.as_array().map(|a| a.len()).unwrap_or(0);
            let data = serde_json::json!({"scanned": count});
            render_envelope(cli.output, OutputEnvelope::success("skill-scan", data))
        }
        SkillSub::Reload => {
            let data: serde_json::Value = client
                .post_json("/api/v1/skills/reload", &serde_json::json!({}))
                .await?;
            let count = data.as_array().map(|a| a.len()).unwrap_or(0);
            let data = serde_json::json!({"reloaded": count});
            render_envelope(cli.output, OutputEnvelope::success("skill-reload", data))
        }
        SkillSub::ClearCache => {
            client
                .post_json::<_, serde_json::Value>(
                    "/api/v1/skills/cache/clear",
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"cleared": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-clear-cache", data),
            )
        }
    }
}
