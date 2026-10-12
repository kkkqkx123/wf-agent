use std::path::Path;

use crate::args::{Cli, ScriptSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ScriptSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        ScriptSub::List => {
            let data: serde_json::Value = client.get_json("/api/v1/scripts").await?;
            render_envelope(cli.output, OutputEnvelope::success("script-list", data))
        }
        ScriptSub::Show { id } => {
            let data: serde_json::Value =
                client.get_json(&format!("/api/v1/scripts/{id}")).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-show", data).with_entity(id.clone()),
            )
        }
        ScriptSub::Validate { name, code } => {
            let body = serde_json::json!({"name": name, "code": code, "args": {}});
            let data: serde_json::Value =
                client.post_json("/api/v1/scripts/validate", &body).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-validate", data).with_entity(name.clone()),
            )
        }
        ScriptSub::Execute {
            name,
            code,
            template,
            args,
        } => {
            let args_map: serde_json::Value = if let Some(json) = args {
                serde_json::from_str(json).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            let body = serde_json::json!({
                "name": name,
                "code": code,
                "template": template,
                "args": args_map,
            });
            let data: serde_json::Value =
                client.post_json("/api/v1/scripts/execute", &body).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-execute", data).with_entity(name.clone()),
            )
        }
        ScriptSub::Save { file } => {
            let meta = load_json(Path::new(file), "script")?;
            let id = meta.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            client.post_json::<_, serde_json::Value>("/api/v1/scripts", &meta).await?;
            let data = serde_json::json!({"saved": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-save", data).with_entity(id),
            )
        }
        ScriptSub::Delete { id, force } => {
            // Reference protection lives server-side: without --force the
            // server rejects referenced scripts instead of cascading.
            let data: serde_json::Value = client
                .delete_json(&format!("/api/v1/scripts/{id}?force={force}"))
                .await?;
            let deleted = data.as_bool().unwrap_or(true);
            if deleted {
                let data = serde_json::json!({"deleted": id});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("script-delete", data).with_entity(id.clone()),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure("script-delete", format!("script not found: {id}")),
                )
            }
        }
        ScriptSub::Enable { id } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/scripts/{id}/enable"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"enabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-enable", data).with_entity(id.clone()),
            )
        }
        ScriptSub::Disable { id } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/scripts/{id}/disable"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"disabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-disable", data).with_entity(id.clone()),
            )
        }
        ScriptSub::Search { query } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/scripts/search?q={query}"))
                .await?;
            render_envelope(cli.output, OutputEnvelope::success("script-search", data))
        }
    }
}

fn load_json(path: &Path, kind: &str) -> CliResult<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content)
        .map_err(|e| CliError::Arguments(format!("invalid {kind} JSON in {}: {e}", path.display())))
}
