//! Trigger domain commands: template registry plus the firing ledger.
//!
//! Thin transport over `wf_api::trigger::{template, execution}`, mirroring
//! the `wf-server` trigger routes. Template arms manage the registry;
//! history/execution arms read the event-driven listener ledger.

use wf_api::trigger::{execution as trigger_execution, template as trigger_template};

use crate::args::{Cli, TriggerSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TriggerSub) -> CliResult<()> {
    let domain =
        crate::domain::DomainHandle::require_embedded(cli, crate::mode::CliMode::Run, "trigger")
            .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let result = match sub {
        TriggerSub::List {
            trigger_type,
            category,
            tags,
            enabled,
            name,
        } => {
            let filter = trigger_template::AgentTriggerTemplateFilter {
                trigger_type: trigger_type.clone(),
                category: category.clone(),
                tags: tags.as_deref().map(|s| {
                    s.split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect()
                }),
                enabled: *enabled,
                name: name.clone(),
            };
            let list = trigger_template::summaries(ctx, Some(&filter)).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("trigger-list", data))
        }
        TriggerSub::Show { id } => {
            let template = trigger_template::get(ctx, id).await?;
            let data = serde_json::to_value(&template)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-show", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Save { file } => {
            let meta = load_trigger_template(std::path::Path::new(file))?;
            trigger_template::save(ctx, &meta).await?;
            let data = serde_json::json!({"saved": meta.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-save", data).with_entity(meta.id.clone()),
            )
        }
        TriggerSub::Delete { id } => {
            let deleted = trigger_template::delete(ctx, id).await?;
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
        TriggerSub::History { execution, trigger } => {
            let history =
                trigger_execution::execution_history(&ctx.storage, execution, trigger.as_deref())
                    .await?;
            let data = serde_json::to_value(&history)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-history", data).with_entity(execution.clone()),
            )
        }
        TriggerSub::Executions {
            trigger,
            execution,
            workflow,
            success,
            limit,
            offset,
        } => {
            let options = wf_api::TriggerExecutionListOptions {
                offset: *offset,
                limit: *limit,
                trigger_name_filter: trigger.clone(),
                execution_id_filter: execution.clone(),
                workflow_id_filter: workflow.clone(),
                success_filter: *success,
            };
            let list =
                trigger_execution::list_trigger_executions(&ctx.storage, Some(options)).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-executions", data),
            )
        }
        TriggerSub::ExecutionShow { id } => {
            let record = trigger_execution::get_trigger_execution(&ctx.storage, id).await?;
            let data = serde_json::to_value(&record)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("trigger-execution-show", data).with_entity(id.clone()),
            )
        }
        TriggerSub::Stats => {
            let stats = trigger_execution::get_trigger_execution_stats(&ctx.storage).await?;
            let data = serde_json::to_value(&stats)?;
            render_envelope(cli.output, OutputEnvelope::success("trigger-stats", data))
        }
        TriggerSub::Cleanup { older_than } => {
            let cutoff = match older_than {
                Some(v) => *v,
                None => wf_api::now(),
            };
            let removed =
                trigger_execution::cleanup_old_trigger_executions(&ctx.storage, cutoff).await?;
            let data = serde_json::json!({"removed": removed, "olderThan": cutoff});
            render_envelope(cli.output, OutputEnvelope::success("trigger-cleanup", data))
        }
    };
    domain.shutdown().await?;
    result
}

fn load_trigger_template(path: &std::path::Path) -> CliResult<wf_api::TriggerTemplateStorageMetadata> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::Arguments(format!("invalid trigger JSON in {}: {e}", path.display()))
    })
}
