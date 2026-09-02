use std::path::Path;

use wf_api::entity::trigger;

use crate::args::{Cli, TriggerSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TriggerSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        TriggerSub::List => {
            let list = trigger::list_triggers(&ctx.storage, None).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("trigger-list", data))
        }
        TriggerSub::Show { id } => {
            let t = trigger::get_trigger(&ctx.storage, id).await?;
            let data = serde_json::to_value(&t)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-show", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Enable { id } => {
            trigger::enable_trigger(&ctx.storage, id).await?;
            let data = serde_json::json!({"enabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-enable", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Disable { id } => {
            trigger::disable_trigger(&ctx.storage, id).await?;
            let data = serde_json::json!({"disabled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-disable", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Register { file } => {
            let t = load_trigger(Path::new(file))?;
            trigger::register_trigger(&ctx.storage, &t).await?;
            let data = serde_json::json!({"registered": t.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-register", data).with_entity(t.id.clone()),
            )
        }
        TriggerSub::Save { file } => {
            let t = load_trigger(Path::new(file))?;
            trigger::save_trigger(&ctx.storage, &t).await?;
            let data = serde_json::json!({"saved": t.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-save", data).with_entity(t.id.clone()),
            )
        }
        TriggerSub::Delete { id } => {
            let deleted = trigger::delete_trigger(&ctx.storage, id).await?;
            if deleted {
                let data = serde_json::json!({"deleted": id});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("trigger-delete", data).with_entity(id.clone()),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure("trigger-delete", format!("trigger not found: {id}")),
                )
            }
        }
        TriggerSub::Search { query } => {
            let list = trigger::search_triggers(&ctx.storage, query).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("trigger-search", data))
        }
        TriggerSub::Stats => {
            let stats = trigger::trigger_statistics(&ctx.storage).await?;
            let data = serde_json::to_value(&stats)?;
            render_envelope(cli.output, OutputEnvelope::success("trigger-stats", data))
        }
    };
    adapter.shutdown().await?;
    result
}

fn load_trigger(path: &Path) -> CliResult<wf_types::TriggerStorageMetadata> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::Arguments(format!("invalid trigger JSON in {}: {e}", path.display()))
    })
}
