use std::path::Path;

use crate::args::{Cli, ToolSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ToolSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        ToolSub::List => {
            let data: serde_json::Value = client.get_json("/api/v1/tools").await?;
            render_envelope(cli.output, OutputEnvelope::success("tool-list", data))
        }
        ToolSub::Show { id } => {
            let data: serde_json::Value =
                client.get_json(&format!("/api/v1/tools/{id}")).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-show", data).with_entity(id.clone()),
            )
        }
        ToolSub::Validate { id, params } => {
            let value: serde_json::Value = serde_json::from_str(params)?;
            // Named request body; execution results stay passthrough JSON
            // because tool registries are externally defined.
            let body = wf_client::models::ValidateToolParamsBody {
                tool_id: id.clone(),
                parameters: value,
            };
            let envelope = client
                .generated()
                .handle_validate_tool_params(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-validate", data).with_entity(id.clone()),
            )
        }
        ToolSub::Execute {
            id,
            params,
            execution_id,
        } => {
            let value: serde_json::Value = serde_json::from_str(params)?;
            let exec_id = execution_id
                .clone()
                .unwrap_or_else(|| "cli-tool-exec".to_string());
            let body = wf_client::models::ExecuteToolBody {
                tool_id: id.clone(),
                parameters: value,
                execution_id: Some(exec_id),
                options: None,
            };
            let envelope = client
                .generated()
                .post_tools_execute(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-execute", data).with_entity(id.clone()),
            )
        }
        ToolSub::Save { file } => {
            let meta = load_json(Path::new(file), "tool")?;
            let id = meta.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            client.post_json::<_, serde_json::Value>("/api/v1/tool-registry", &meta).await?;
            let data = serde_json::json!({"saved": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-save", data).with_entity(id),
            )
        }
        ToolSub::Delete { id, force } => {
            // Reference protection lives server-side: without --force the
            // server rejects referenced tools instead of cascading.
            let data: serde_json::Value = client
                .delete_json(&format!("/api/v1/tool-registry/{id}?force={force}"))
                .await?;
            let deleted = data.as_bool().unwrap_or(true);
            if deleted {
                let data = serde_json::json!({"deleted": id});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("tool-delete", data).with_entity(id.clone()),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure("tool-delete", format!("tool not found: {id}")),
                )
            }
        }
        ToolSub::Enable { id } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/tools/{id}/enable"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"enabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-enable", data).with_entity(id.clone()),
            )
        }
        ToolSub::Disable { id } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/tools/{id}/disable"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"disabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-disable", data).with_entity(id.clone()),
            )
        }
        ToolSub::Search { query } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/tools/search?q={query}"))
                .await?;
            render_envelope(cli.output, OutputEnvelope::success("tool-search", data))
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
