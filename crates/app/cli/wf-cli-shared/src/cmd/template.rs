use std::path::Path;

use crate::args::{Cli, TemplateSub};
use crate::cmd::render::render_envelope;
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
            let mut params: Vec<String> = Vec::new();
            if let Some(k) = kind {
                match k.to_ascii_lowercase().as_str() {
                    "workflow" | "agent" => params.push(format!("kind={k}")),
                    _ => {}
                }
            }
            if let Some(cat) = category {
                params.push(format!("category={cat}"));
            }
            if let Some(t) = tags {
                params.push(format!("tags={t}"));
            }
            if let Some(a) = author {
                params.push(format!("author={a}"));
            }
            let path = if params.is_empty() {
                "/api/v1/templates/library".to_string()
            } else {
                format!("/api/v1/templates/library?{}", params.join("&"))
            };
            let data: serde_json::Value = client.get_json(&path).await?;
            render_envelope(cli.output, OutputEnvelope::success("template-list", data))
        }
        TemplateSub::Show { id, kind: _ } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/templates/library?name={id}"))
                .await?;
            let found = data
                .as_array()
                .and_then(|items| {
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
                None => render_envelope(
                    cli.output,
                    OutputEnvelope::failure("template-show", format!("template not found: {id}")),
                ),
            }
        }
        TemplateSub::Clone { id } => {
            let new_name = format!("{id}-clone");
            let body = serde_json::json!({"kind": "workflow", "new_name": new_name});
            let data: serde_json::Value = client
                .post_json(&format!("/api/v1/templates/library/{id}/clone"), &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("template-clone", data).with_entity(id.clone()),
            )
        }
        TemplateSub::Register { file, kind, format } => {
            let path = Path::new(file);
            let fmt = resolve_format(path, format);
            match kind.to_ascii_lowercase().as_str() {
                "agent" => {
                    let template = load_agent_template(path, &fmt)?;
                    let id = template
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    client
                        .post_json::<_, serde_json::Value>("/api/v1/templates/library/agents", &template)
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
                    client
                        .post_json::<_, serde_json::Value>("/api/v1/templates/library/workflows", &template)
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
            "agent" => {
                client
                    .delete_json::<serde_json::Value>(&format!("/api/v1/templates/library/agents/{id}"))
                    .await?;
                let data = serde_json::json!({"deleted": id, "kind": "agent"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
            _ => {
                client
                    .delete_json::<serde_json::Value>(&format!("/api/v1/templates/library/workflows/{id}"))
                    .await?;
                let data = serde_json::json!({"deleted": id, "kind": "workflow"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
        },
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
