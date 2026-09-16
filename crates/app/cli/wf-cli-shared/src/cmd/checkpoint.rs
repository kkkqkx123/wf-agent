use wf_api::checkpoint::record as checkpoint;
use wf_api::workflow::workflow_execution;

use crate::args::{CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

fn domain_override(
    domain: Option<crate::args::CheckpointDomain>,
) -> Option<wf_api::ExecutionDomain> {
    domain.map(wf_api::ExecutionDomain::from)
}

pub async fn run(cli: &Cli, sub: &CheckpointSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        CheckpointSub::Create { id } => {
            let checkpoint_id = workflow_execution::create_checkpoint(ctx, id).await?;
            let data = serde_json::json!({"executionId": id, "checkpointId": checkpoint_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-create", data)
                    .with_entity(checkpoint_id.clone()),
            )
        }
        CheckpointSub::CreateAgent { id, name } => {
            let created =
                wf_api::agent::agent_checkpoint::create(ctx, id, name.clone()).await?;
            let data =
                serde_json::json!({"agentLoopId": id, "checkpointId": created.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-create", data)
                    .with_entity(created.id.clone()),
            )
        }
        CheckpointSub::FileCreate { id, path } => {
            let manager = ctx.file_checkpoint_manager().ok_or_else(|| {
                crate::error::CliError::Business(
                    "file checkpointing is not enabled; set file_checkpoint.enabled=true".to_string(),
                )
            })?;
            let summary = wf_api::checkpoint::file::create_file_checkpoint(
                manager,
                id,
                std::path::Path::new(path),
            )?;
            let data = serde_json::to_value(&summary)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-file-create", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::List {
            id,
            limit,
            offset,
            domain,
        } => {
            let mut list =
                checkpoint::list_for_execution(ctx, id, domain_override(*domain)).await?;
            list.sort_by_key(|c| c.timestamp);
            let off = offset.unwrap_or(0);
            let lim = limit.unwrap_or(usize::MAX);
            let paged: Vec<_> = list.into_iter().skip(off).take(lim).collect();
            let data = serde_json::to_value(&paged)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-list", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Show { id } => {
            let cp = checkpoint::get_checkpoint(&ctx.storage, id).await?;
            let data = serde_json::to_value(&cp)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-show", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Restore { id, resume } => {
            let restored = workflow_execution::restore_checkpoint(ctx, id).await?;
            if *resume {
                let output = workflow_execution::resume(ctx, &restored.execution_id).await?;
                let data = serde_json::json!({"restored": id, "executionId": restored.execution_id, "result": output.result});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore-resume", data)
                        .with_entity(restored.execution_id.clone()),
                )
            } else {
                let data = serde_json::to_value(&restored)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("checkpoint-restore", data)
                        .with_entity(restored.execution_id.clone()),
                )
            }
        }
        CheckpointSub::RestoreAgent { id, checkpoint } => {
            let restored =
                wf_api::agent::agent_checkpoint::restore(ctx, id, checkpoint).await?;
            let data = serde_json::to_value(&restored)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-restore", data)
                    .with_entity(restored.id.clone()),
            )
        }
        CheckpointSub::Delete { id } => {
            let deleted = checkpoint::delete_checkpoint(&ctx.storage, id).await?;
            let data = serde_json::json!({"deleted": id, "ok": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-delete", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Chain { id, domain } => {
            let chain =
                checkpoint::chain_for_execution(ctx, id, domain_override(*domain)).await?;
            let data = serde_json::to_value(&chain)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-chain", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Gc { id, before, domain } => {
            let deleted =
                checkpoint::gc_for_execution(ctx, id, *before, domain_override(*domain)).await?;
            let data = serde_json::json!({"executionId": id, "deleted": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-gc", data).with_entity(id.clone()),
            )
        }
    };
    adapter.shutdown().await?;
    result
}
