use wf_api::checkpoint::record as checkpoint;
use wf_api::workflow::workflow_execution;

use crate::args::{CheckpointSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &CheckpointSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        CheckpointSub::Create { id, name: _ } => {
            let checkpoint_id = workflow_execution::create_checkpoint(ctx, id).await?;
            let data = serde_json::json!({"executionId": id, "checkpointId": checkpoint_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-create", data)
                    .with_entity(checkpoint_id.clone()),
            )
        }
        CheckpointSub::List { id, limit, offset } => {
            let mut list =
                checkpoint::list_checkpoints_by_entity(&ctx.storage, id, "checkpoint").await?;
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
        CheckpointSub::Delete { id } => {
            let deleted = checkpoint::delete_checkpoint(&ctx.storage, id).await?;
            let data = serde_json::json!({"deleted": id, "ok": deleted});
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-delete", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Chain { id } => {
            let chain = checkpoint::get_checkpoint_chain(&ctx.storage, id).await?;
            let data = serde_json::to_value(&chain)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("checkpoint-chain", data).with_entity(id.clone()),
            )
        }
        CheckpointSub::Gc { id, before } => {
            let deleted = if let Some(ts) = before {
                // Filter by timestamp before deleting.
                let list =
                    checkpoint::list_checkpoints_by_entity(&ctx.storage, id, "checkpoint").await?;
                let mut count = 0u64;
                for cp in list {
                    if cp.timestamp < *ts
                        && checkpoint::delete_checkpoint(&ctx.storage, &cp.id).await?
                    {
                        count += 1;
                    }
                }
                count
            } else {
                checkpoint::delete_checkpoints_by_entity(&ctx.storage, id, "checkpoint").await?
            };
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
