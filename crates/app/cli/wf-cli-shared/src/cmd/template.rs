use std::path::Path;

use wf_api::template::template_library;

use crate::args::{Cli, TemplateSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &TemplateSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        TemplateSub::List {
            kind,
            category,
            tags,
            author,
        } => {
            let mut filter = template_library::TemplateFilter::default();
            if let Some(k) = kind {
                match k.to_ascii_lowercase().as_str() {
                    "workflow" => filter.kind = Some(template_library::TemplateKind::Workflow),
                    "agent" => filter.kind = Some(template_library::TemplateKind::Agent),
                    _ => {}
                }
            }
            if let Some(cat) = category {
                filter.category = Some(cat.clone());
            }
            if let Some(t) = tags {
                filter.tags = Some(
                    t.split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect(),
                );
            }
            if let Some(a) = author {
                filter.author = Some(a.clone());
            }
            let list = template_library::query(ctx, &filter)?;
            let data = serde_json::to_value(&list)?;
            render_envelope(cli.output, OutputEnvelope::success("template-list", data))
        }
        TemplateSub::Show { id, kind: _ } => {
            let filter = template_library::TemplateFilter {
                name: Some(id.clone()),
                ..Default::default()
            };
            let list = template_library::query(ctx, &filter)?;
            if let Some(t) = list.into_iter().find(|t| t.id == *id || t.name == *id) {
                let data = serde_json::to_value(&t)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-show", data).with_entity(id.clone()),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure("template-show", format!("template not found: {id}")),
                )
            }
        }
        TemplateSub::Clone { id } => {
            let new_name = format!("{id}-clone");
            let cloned = template_library::clone_workflow_template(ctx, id, &new_name)?;
            let data = serde_json::to_value(&cloned)?;
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
                    template_library::register_agent_template(ctx, &template).await?;
                    let data = serde_json::json!({"registered": template.id, "kind": "agent"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("template-register", data)
                            .with_entity(template.id.to_string()),
                    )
                }
                _ => {
                    let template = load_workflow_template(path, &fmt)?;
                    template_library::register_workflow_template(ctx, &template)?;
                    let data = serde_json::json!({"registered": template.id, "kind": "workflow"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("template-register", data)
                            .with_entity(template.id.to_string()),
                    )
                }
            }
        }
        TemplateSub::Delete { id, kind } => match kind.to_ascii_lowercase().as_str() {
            "agent" => {
                template_library::delete_agent_template(ctx, id).await?;
                let data = serde_json::json!({"deleted": id, "kind": "agent"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
            _ => {
                template_library::delete_workflow_template(ctx, id)?;
                let data = serde_json::json!({"deleted": id, "kind": "workflow"});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("template-delete", data).with_entity(id.clone()),
                )
            }
        },
    };
    adapter.shutdown().await?;
    result
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

fn load_workflow_template(
    path: &Path,
    fmt: &str,
) -> CliResult<wf_types::workflow::WorkflowTemplate> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    if fmt == "toml" {
        if let Ok(t) = toml::from_str::<wf_types::workflow::WorkflowTemplate>(&content) {
            return Ok(t);
        }
        let def: wf_types::WorkflowDefinition = toml::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display())))?;
        Ok(wf_types::workflow::WorkflowTemplate {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone().unwrap_or_default(),
            definition: def,
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        })
    } else {
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid JSON in {}: {e}", path.display())))?;
        if let Ok(t) = serde_json::from_value::<wf_types::workflow::WorkflowTemplate>(value.clone())
        {
            return Ok(t);
        }
        let def: wf_types::WorkflowDefinition = serde_json::from_value(value)
            .map_err(|e| CliError::Arguments(format!("workflow parse failed: {e}")))?;
        Ok(wf_types::workflow::WorkflowTemplate {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone().unwrap_or_default(),
            definition: def,
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        })
    }
}

fn load_agent_template(path: &Path, fmt: &str) -> CliResult<wf_types::agent::AgentTemplate> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    if fmt == "toml" {
        if let Ok(t) = toml::from_str::<wf_types::agent::AgentTemplate>(&content) {
            return Ok(t);
        }
        let def: wf_types::agent::AgentDefinition = toml::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display())))?;
        Ok(wf_types::agent::AgentTemplate {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone().unwrap_or_default(),
            definition: def,
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        })
    } else {
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid JSON in {}: {e}", path.display())))?;
        if let Ok(t) = serde_json::from_value::<wf_types::agent::AgentTemplate>(value.clone()) {
            return Ok(t);
        }
        let def: wf_types::agent::AgentDefinition = serde_json::from_value(value)
            .map_err(|e| CliError::Arguments(format!("agent parse failed: {e}")))?;
        Ok(wf_types::agent::AgentTemplate {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone().unwrap_or_default(),
            definition: def,
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        })
    }
}
