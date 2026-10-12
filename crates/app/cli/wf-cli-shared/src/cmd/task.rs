use crate::args::{Cli, TaskSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TaskSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        TaskSub::List {
            status,
            task_type,
            limit,
        } => {
            let mut params: Vec<String> = Vec::new();
            if let Some(s) = status {
                params.push(format!("status={s}"));
            }
            if let Some(t) = task_type {
                params.push(format!("task_type={t}"));
            }
            if let Some(l) = limit {
                params.push(format!("limit={l}"));
            }
            let path = if params.is_empty() {
                "/api/v1/tasks".to_string()
            } else {
                format!("/api/v1/tasks?{}", params.join("&"))
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(cli.output, OutputEnvelope::success("task-list", data))
        }
        TaskSub::Show { id } => {
            let data: serde_json::Value =
                client.get_json(&format!("/api/v1/tasks/{id}")).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("task-show", data).with_entity(id.clone()),
            )
        }
        TaskSub::Stats => {
            let data: serde_json::Value = client.get_json("/api/v1/tasks/stats").await?;
            render_envelope(cli.output, OutputEnvelope::success("task-stats", data))
        }
        TaskSub::Cancel { id } => {
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/tasks/{id}/cancel"),
                    &serde_json::json!({}),
                )
                .await?;
            let data = serde_json::json!({"cancelled": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("task-cancel", data).with_entity(id.clone()),
            )
        }
    }
}
