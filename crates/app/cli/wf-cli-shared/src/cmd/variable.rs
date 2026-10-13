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
            let envelope = client
                .generated()
                .get_variables(
                    Some(execution.as_str()),
                    None,
                    None,
                    scope.as_deref(),
                )
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
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
            let envelope = client
                .generated()
                .get_variables_name(name, Some(execution.as_str()), Some(scope.as_str()))
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
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
            // Named request body; response stays passthrough JSON.
            let body = wf_client::models::VariableBody {
                name: name.clone(),
                value: val,
                scope: Some(scope.clone()),
                execution_id: Some(execution.clone()),
                define: None,
            };
            let envelope = client
                .generated()
                .post_variables(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let _: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
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
            let envelope = client
                .generated()
                .delete_variables_name(name, Some(execution.as_str()), Some(scope.as_str()))
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let deleted: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let deleted = deleted.as_bool().unwrap_or(true);
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
