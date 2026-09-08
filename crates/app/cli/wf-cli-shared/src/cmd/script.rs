use std::path::Path;

use wf_api::llm::script;

use crate::args::{Cli, ScriptSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ScriptSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        ScriptSub::List => {
            let list = script::list_scripts(&ctx.storage, None).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("script-list", data))
        }
        ScriptSub::Show { id } => {
            let s = script::get_script(&ctx.storage, id).await?;
            let data = serde_json::to_value(&s)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-show", data).with_entity(id.clone()),
            )
        }
        ScriptSub::Validate { name, code } => {
            let params = script::ScriptExecuteParams {
                name: name.clone(),
                code: code.clone(),
                ..Default::default()
            };
            let v = script::validate(ctx, &params).await?;
            let data = serde_json::to_value(&v)?;
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
            let args_map = if let Some(json) = args {
                serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(json)
                    .unwrap_or_default()
            } else {
                std::collections::HashMap::new()
            };
            let params = script::ScriptExecuteParams {
                name: name.clone(),
                code: code.clone(),
                template: template.clone(),
                args: args_map,
                ..Default::default()
            };
            let result = script::execute(ctx, &params).await?;
            let data = serde_json::to_value(&result)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-execute", data).with_entity(name.clone()),
            )
        }
        ScriptSub::Save { file } => {
            let meta = load_script(Path::new(file))?;
            script::save_script(&ctx.storage, &meta).await?;
            let data = serde_json::json!({"saved": meta.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-save", data).with_entity(meta.id.clone()),
            )
        }
        ScriptSub::Delete { id, force } => {
            if !*force {
                let refs = script::check_script_delete_references(&ctx.storage, id).await?;
                if !refs.is_empty() {
                    let list = refs
                        .iter()
                        .map(|r| format!("{}#{}", r.workflow_id, r.node_id))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let msg = format!(
                        "script '{}' is referenced by: {list} (use --force to cascade)",
                        id
                    );
                    return render_envelope(
                        cli.output,
                        OutputEnvelope::failure("script-delete", msg),
                    );
                }
            }
            let deleted = script::delete_script(&ctx.storage, id).await?;
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
            script::enable_script(&ctx.storage, id).await?;
            let data = serde_json::json!({"enabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-enable", data).with_entity(id.clone()),
            )
        }
        ScriptSub::Disable { id } => {
            script::disable_script(&ctx.storage, id).await?;
            let data = serde_json::json!({"disabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("script-disable", data).with_entity(id.clone()),
            )
        }
        ScriptSub::Search { query } => {
            let list = script::search_scripts(&ctx.storage, query).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("script-search", data))
        }
    };
    adapter.shutdown().await?;
    result
}

fn load_script(path: &Path) -> CliResult<wf_types::ScriptStorageMetadata> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content)
        .map_err(|e| CliError::Arguments(format!("invalid script JSON in {}: {e}", path.display())))
}
