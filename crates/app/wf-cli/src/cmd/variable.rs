use wf_api::entity::variable;

use crate::args::{Cli, VariableSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &VariableSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        VariableSub::List { execution, scope } => {
            let vars = if let Some(scope) = scope {
                variable::list(
                    ctx,
                    &wf_api::VariableListOptions {
                        execution_id_filter: Some(execution.clone()),
                        scope_filter: Some(scope.clone()),
                        ..Default::default()
                    },
                )
                .await?
            } else {
                variable::list_by_execution(ctx, execution).await?
            };
            let data = serde_json::to_value(&vars)?;
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
            let v = variable::get(ctx, name, scope, Some(execution)).await?;
            let data = serde_json::to_value(&v)?;
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
            variable::set(ctx, name, scope, Some(execution), val).await?;
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
            let deleted = variable::delete(ctx, name, scope, Some(execution)).await?;
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
    };
    adapter.shutdown().await?;
    result
}
