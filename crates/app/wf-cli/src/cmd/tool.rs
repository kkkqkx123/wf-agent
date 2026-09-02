use std::path::Path;

use wf_api::llm::tool;

use crate::args::{Cli, ToolSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ToolSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        ToolSub::List => {
            let tools = tool::list(ctx).await?;
            let data = serde_json::to_value(&tools)?;
            render_envelope(cli.output, OutputEnvelope::success("tool-list", data))
        }
        ToolSub::Show { id } => {
            let t = tool::get(ctx, id).await?;
            let data = serde_json::to_value(&t)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-show", data).with_entity(id.clone()),
            )
        }
        ToolSub::Validate { id, params } => {
            let value: serde_json::Value = serde_json::from_str(params)?;
            let v = tool::validate_parameters(ctx, id, &value).await?;
            let data = serde_json::to_value(&v)?;
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
            let result = tool::execute(ctx, id, &value, None, &exec_id).await?;
            let data = serde_json::to_value(&result)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-execute", data).with_entity(id.clone()),
            )
        }
        ToolSub::Save { file } => {
            let meta = load_tool(Path::new(file))?;
            tool::save_tool(&ctx.storage, &meta).await?;
            let data = serde_json::json!({"saved": meta.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-save", data).with_entity(meta.id.clone()),
            )
        }
        ToolSub::Delete { id, force } => {
            if !*force {
                let refs = tool::check_delete_references(&ctx.storage, id).await?;
                if !refs.is_empty() {
                    let list = refs
                        .iter()
                        .map(|r| format!("{}#{}", r.workflow_id, r.node_id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let msg = format!(
                        "tool '{}' is referenced by: {list} (use --force to cascade)",
                        id
                    );
                    return render_envelope(
                        cli.output,
                        OutputEnvelope::failure("tool-delete", msg),
                    );
                }
            }
            let deleted = tool::delete_tool(&ctx.storage, id).await?;
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
            tool::enable(ctx, id).await?;
            let data = serde_json::json!({"enabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-enable", data).with_entity(id.clone()),
            )
        }
        ToolSub::Disable { id } => {
            tool::disable(ctx, id).await?;
            let data = serde_json::json!({"disabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("tool-disable", data).with_entity(id.clone()),
            )
        }
        ToolSub::Search { query } => {
            let list = tool::search_tools(ctx, query).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("tool-search", data))
        }
    };
    adapter.shutdown().await?;
    result
}

fn load_tool(path: &Path) -> CliResult<wf_types::ToolStorageMetadata> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content)
        .map_err(|e| CliError::Arguments(format!("invalid tool JSON in {}: {e}", path.display())))
}
