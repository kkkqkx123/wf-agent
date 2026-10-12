use crate::args::{Cli, VariableSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &VariableSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        VariableSub::List { execution, scope } => {
            let mut path = format!("/api/v1/variables?execution_id={execution}");
            if let Some(scope) = scope {
                path.push_str(&format!("&scope={scope}"));
            }
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("variable-list", data).with_entity(execution.clone()),
            )
        }
        VariableSub::Get {
            execution,
            scope,
            name,
        } => {
            let data: serde_json::Value = client
                .get_json(&format!(
                    "/api/v1/variables/{name}?scope={scope}&execution_id={execution}"
                ))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("variable-get", data)
                    .with_entity(format!("{execution}:{scope}:{name}")),
            )
        }
        VariableSub::Set {
            execution,
            scope,
            name,
            value,
        } => {
            let val: serde_json::Value =
                serde_json::from_str(value).unwrap_or(serde_json::Value::String(value.clone()));
            let body = serde_json::json!({
                "name": name,
                "value": val,
                "scope": scope,
                "execution_id": execution,
            });
            client.post_json::<_, serde_json::Value>("/api/v1/variables", &body).await?;
            let data = serde_json::json!({"execution": execution, "scope": scope, "name": name, "set": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("variable-set", data)
                    .with_entity(format!("{execution}:{scope}:{name}")),
            )
        }
        VariableSub::Delete {
            execution,
            scope,
            name,
        } => {
            let deleted: bool = client
                .delete_json(&format!(
                    "/api/v1/variables/{name}?scope={scope}&execution_id={execution}"
                ))
                .await?;
            let data = serde_json::json!({"deleted": deleted, "name": name});
            if deleted {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("variable-delete", data)
                        .with_entity(format!("{execution}:{scope}:{name}")),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure(
                        "variable-delete",
                        format!("variable not found: {name}"),
                    ),
                )
            }
        }
    }
}
