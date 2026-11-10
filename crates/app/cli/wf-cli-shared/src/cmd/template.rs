use std::path::Path;

use crate::args::{Cli, TemplateSub, TemplateVersionSub};
use crate::cmd::render::{render_envelope, render_failure};
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TemplateSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        TemplateSub::List {
            kind,
            category,
            tags,
            author,
        } => {
            let kind_param = kind.as_deref().map(|k| k.to_ascii_lowercase()).filter(|k| {
                matches!(
                    k.as_str(),
                    "workflow" | "agent" | "node" | "trigger" | "all"
                )
            });
            let envelope = client
                .generated()
                .handle_query_library(
                    author.as_deref(),
                    category.as_deref(),
                    None,
                    kind_param.as_deref(),
                    None,
                    tags.as_deref(),
                )
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            render_envelope(cli.output, OutputEnvelope::success("template-list", data))
        }
        TemplateSub::Show { id, kind } => {
            let kind_param = kind
                .as_deref()
                .map(|k| k.to_ascii_lowercase())
                .filter(|k| matches!(k.as_str(), "workflow" | "agent" | "node" | "trigger"));
            let envelope = client
                .generated()
                .handle_query_library(None, None, None, kind_param.as_deref(), None, None)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let found = data.as_array().and_then(|items| {
                items
                    .iter()
                    .find(|t| {
                        t.get("id").and_then(|v| v.as_str()) == Some(id.as_str())
                            || t.get("name").and_then(|v| v.as_str()) == Some(id.as_str())
                    })
                    .cloned()
            });
            match found {
                Some(t) => render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-show", t).with_entity(id.clone()),
                ),
                None => render_failure(
                    cli.output,
                    "template-show",
                    format!("template not found: {id}"),
                ),
            }
        }
        TemplateSub::Clone { id, kind } => {
            let kind_value = kind.to_ascii_lowercase();
            let new_name = format!("{id}-clone");
            let body = wf_client::models::CloneTemplateBody {
                kind: kind_value.clone(),
                new_name: Some(new_name),
            };
            let envelope = client
                .generated()
                .handle_clone_template(id, &body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("template-clone", data).with_entity(id.clone()),
            )
        }
        TemplateSub::Register { file, kind, format } => {
            let path = Path::new(file);
            let fmt = resolve_format(path, format);
            match kind.to_ascii_lowercase().as_str() {
                "node" => {
                    let template = load_raw_template(path, &fmt)?;
                    let id = template
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    client
                        .post_json::<_, serde_json::Value>("/api/v1/templates/node", &template)
                        .await?;
                    let data = serde_json::json!({"registered": id, "kind": "node"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("template-register", data).with_entity(id),
                    )
                }
                "trigger" => {
                    let template = load_raw_template(path, &fmt)?;
                    let id = template
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    client
                        .post_json::<_, serde_json::Value>("/api/v1/templates/trigger", &template)
                        .await?;
                    let data = serde_json::json!({"registered": id, "kind": "trigger"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("template-register", data).with_entity(id),
                    )
                }
                "agent" => {
                    let template = load_agent_template(path, &fmt)?;
                    let id = template
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    client
                        .post_json::<_, serde_json::Value>("/api/v1/agent-definitions", &template)
                        .await?;
                    let data = serde_json::json!({"registered": id, "kind": "agent"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("template-register", data).with_entity(id),
                    )
                }
                _ => {
                    let template = load_workflow_template(path, &fmt)?;
                    let id = template
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    // The definition surface owns workflow rows; a template
                    // file carries the definition inside the wrapper.
                    let definition = template
                        .get("definition")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    client
                        .post_json::<_, serde_json::Value>("/api/v1/workflows", &definition)
                        .await?;
                    let data = serde_json::json!({"registered": id, "kind": "workflow"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("template-register", data).with_entity(id),
                    )
                }
            }
        }
        TemplateSub::Delete { id, kind } => match kind.to_ascii_lowercase().as_str() {
            "node" => {
                let _: serde_json::Value = client
                    .delete_json(&format!("/api/v1/templates/node/{id}"))
                    .await?;
                let data = serde_json::json!({"deleted": id, "kind": "node"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
            "trigger" => {
                let _: serde_json::Value = client
                    .delete_json(&format!("/api/v1/templates/trigger/{id}"))
                    .await?;
                let data = serde_json::json!({"deleted": id, "kind": "trigger"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
            "agent" => {
                let envelope = client
                    .generated()
                    .handle_delete_agent_template(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let _: serde_json::Value =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::json!({"deleted": id, "kind": "agent"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
            _ => {
                // Removing a workflow from the library leaves the workflow
                // in place: the published mark is cleared through the
                // definition surface.
                let _: serde_json::Value = client
                    .post_json(
                        &format!("/api/v1/workflows/{id}/unpublish"),
                        &serde_json::json!({}),
                    )
                    .await?;
                let data = serde_json::json!({"removed_from_library": id, "kind": "workflow"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
        },
        TemplateSub::Version { sub } => match sub {
            TemplateVersionSub::List { id } => {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/agent-definitions/{id}/versions"))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-version-list", data).with_entity(id.clone()),
                )
            }
            TemplateVersionSub::Show {
                id,
                version_label: version,
            } => {
                let data: serde_json::Value = client
                    .get_json(&format!(
                        "/api/v1/agent-definitions/{id}/versions/{version}"
                    ))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-version-show", data)
                        .with_entity(format!("{id}:{version}")),
                )
            }
        },
        TemplateSub::Rollback {
            id,
            target_version: version,
        } => {
            let body = serde_json::json!({ "version": version });
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/agent-definitions/{id}/rollback"),
                    &body,
                )
                .await?;
            let data = serde_json::json!({"agentDefinitionId": id, "rolledBackTo": version});
            render_envelope(
                cli.output,
                OutputEnvelope::success("template-rollback", data).with_entity(id.clone()),
            )
        }
    }
}

fn load_raw_template(path: &Path, fmt: &str) -> CliResult<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    if fmt == "toml" {
        toml::from_str::<serde_json::Value>(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display())))
    } else {
        serde_json::from_str::<serde_json::Value>(&content)
            .map_err(|e| CliError::Arguments(format!("invalid JSON in {}: {e}", path.display())))
    }
}

fn resolve_format(path: &Path, format: &str) -> String {
    if format != "auto" {
        return format.to_ascii_lowercase();
    }
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("toml") => "toml".to_string(),
        _ => "json".to_string(),
    }
}

fn load_workflow_template(path: &Path, fmt: &str) -> CliResult<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    if fmt == "toml" {
        if let Ok(t) = toml::from_str::<serde_json::Value>(&content) {
            if t.get("definition").is_some() {
                return Ok(t);
            }
        }
        let def: wf_types::WorkflowDefinition = toml::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display())))?;
        Ok(serde_json::json!({
            "id": def.id.clone(),
            "name": def.name.clone(),
            "description": def.description.clone().unwrap_or_default(),
            "definition": def,
        }))
    } else {
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid JSON in {}: {e}", path.display())))?;
        if value.get("definition").is_some() {
            return Ok(value);
        }
        let def: wf_types::WorkflowDefinition = serde_json::from_value(value)
            .map_err(|e| CliError::Arguments(format!("workflow parse failed: {e}")))?;
        Ok(serde_json::json!({
            "id": def.id.clone(),
            "name": def.name.clone(),
            "description": def.description.clone().unwrap_or_default(),
            "definition": def,
        }))
    }
}

fn load_agent_template(path: &Path, fmt: &str) -> CliResult<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    if fmt == "toml" {
        if let Ok(t) = toml::from_str::<serde_json::Value>(&content) {
            if t.get("definition").is_some() {
                return Ok(t);
            }
        }
        let def: wf_types::agent::AgentDefinition = toml::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display())))?;
        Ok(serde_json::json!({
            "id": def.id.clone(),
            "name": def.name.clone(),
            "description": def.description.clone().unwrap_or_default(),
            "definition": def,
        }))
    } else {
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid JSON in {}: {e}", path.display())))?;
        if value.get("definition").is_some() {
            return Ok(value);
        }
        let def: wf_types::agent::AgentDefinition = serde_json::from_value(value)
            .map_err(|e| CliError::Arguments(format!("agent parse failed: {e}")))?;
        Ok(serde_json::json!({
            "id": def.id.clone(),
            "name": def.name.clone(),
            "description": def.description.clone().unwrap_or_default(),
            "definition": def,
        }))
    }
}
