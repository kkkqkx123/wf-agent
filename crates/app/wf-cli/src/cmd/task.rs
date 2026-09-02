use wf_api::entity::task;
use wf_storage::adapter::task::TaskListOptions;

use crate::args::{Cli, TaskSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TaskSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        TaskSub::List {
            status,
            task_type,
            limit,
        } => {
            let opts = if status.is_some() || task_type.is_some() || limit.is_some() {
                Some(TaskListOptions {
                    status_filter: status.clone(),
                    task_type_filter: task_type.clone(),
                    limit: limit.map(|v| v as u64),
                    offset: None,
                })
            } else {
                None
            };
            let list = task::list_tasks(&ctx.storage, opts).await?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("task-list", data))
        }
        TaskSub::Show { id } => {
            let t = task::get_task(&ctx.storage, id).await?;
            let data = serde_json::to_value(&t)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("task-show", data).with_entity(id.clone()),
            )
        }
        TaskSub::Stats => {
            let stats = task::get_task_stats(&ctx.storage).await?;
            let data = serde_json::to_value(&stats)?;
            render_envelope(cli.output, OutputEnvelope::success("task-stats", data))
        }
        TaskSub::Cancel { id } => {
            task::cancel_task(&ctx.storage, id).await?;
            let data = serde_json::json!({"cancelled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("task-cancel", data).with_entity(id.clone()),
            )
        }
    };
    adapter.shutdown().await?;
    result
}
