use std::path::Path;

use wf_api::llm::llm_profile;

use crate::args::{Cli, LlmProfileSub, LlmTemplateSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &LlmProfileSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let result = match sub {
        LlmProfileSub::List => {
            let profiles = llm_profile::list(ctx).await?;
            let data = serde_json::to_value(&profiles)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-list", data),
            )
        }
        LlmProfileSub::Show { id } => {
            let profile = llm_profile::get(ctx, id).await?;
            let data = serde_json::to_value(&profile)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-show", data).with_entity(id.clone()),
            )
        }
        LlmProfileSub::Create { file } => {
            let profile = load_profile(Path::new(file))?;
            llm_profile::create(ctx, &profile).await?;
            let data = serde_json::json!({"id": profile.id, "created": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-create", data).with_entity(profile.id),
            )
        }
        LlmProfileSub::Update { id, file } => {
            let mut profile = load_profile(Path::new(file))?;
            profile.id = id.clone();
            llm_profile::update(ctx, &profile).await?;
            let data = serde_json::to_value(&profile)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-update", data).with_entity(id.clone()),
            )
        }
        LlmProfileSub::Delete { id } => {
            llm_profile::delete(ctx, id).await?;
            let data = serde_json::json!({"deleted": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-delete", data).with_entity(id.clone()),
            )
        }
        LlmProfileSub::Validate { file } => {
            let profile = load_profile(Path::new(file))?;
            let (valid, errors) = llm_profile::validate(ctx, &profile);
            let data = serde_json::json!({"valid": valid, "errors": errors, "id": profile.id});
            let envelope = OutputEnvelope::success("llm-profile-validate", data);
            render_envelope(cli.output, envelope)
        }
        LlmProfileSub::Default { set } => {
            if let Some(id) = set {
                llm_profile::set_default(ctx, id).await?;
                let data = serde_json::json!({"default": id});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-default-set", data)
                        .with_entity(id.clone()),
                )
            } else {
                let def = llm_profile::get_default(ctx).await?;
                let data = serde_json::to_value(&def)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-default", data),
                )
            }
        }
        LlmProfileSub::Template { sub } => match sub {
            LlmTemplateSub::List {
                kind,
                category,
                tags,
                author,
            } => {
                let templates = llm_profile::list_templates(ctx).await?;
                let filtered: Vec<_> = templates
                    .into_iter()
                    .filter(|t| {
                        if let Some(k) = kind {
                            let k_lower = k.to_lowercase();
                            if !t.name.to_lowercase().contains(&k_lower)
                                && !t.description.to_lowercase().contains(&k_lower)
                                && !format!("{:?}", t.profile.provider)
                                    .to_lowercase()
                                    .contains(&k_lower)
                            {
                                return false;
                            }
                        }
                        if let Some(cat) = category {
                            if !t.description.to_lowercase().contains(&cat.to_lowercase())
                                && !t.name.to_lowercase().contains(&cat.to_lowercase())
                            {
                                return false;
                            }
                        }
                        if let Some(tgs) = tags {
                            let wanted: Vec<String> = tgs
                                .split(',')
                                .map(|v| v.trim().to_lowercase())
                                .filter(|v| !v.is_empty())
                                .collect();
                            if !wanted.is_empty() {
                                let hay = format!("{} {}", t.name, t.description).to_lowercase();
                                if !wanted.iter().any(|w| hay.contains(w)) {
                                    return false;
                                }
                            }
                        }
                        if let Some(a) = author {
                            if !t.description.to_lowercase().contains(&a.to_lowercase())
                                && !t.name.to_lowercase().contains(&a.to_lowercase())
                            {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();
                let data = serde_json::to_value(&filtered)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-template-list", data),
                )
            }
        },
        LlmProfileSub::Export { id, file } => {
            let value = llm_profile::export(ctx, id).await?;
            if cli.output == crate::output::OutputFormat::Text {
                eprintln!("warning: api_key masked as {}", wf_api::MASKED_API_KEY);
            }
            if let Some(path) = file {
                std::fs::write(path, serde_json::to_string_pretty(&value)?)
                    .map_err(|e| CliError::Configuration(format!("write failed: {e}")))?;
                let data = serde_json::json!({"exported": id, "output": path, "masked": true});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-export", data).with_entity(id.clone()),
                )
            } else {
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-export", value).with_entity(id.clone()),
                )
            }
        }
        LlmProfileSub::Import { file } => {
            let content = std::fs::read_to_string(file)
                .map_err(|e| CliError::Configuration(format!("read failed: {e}")))?;
            let id = llm_profile::import_json(ctx, &content).await?;
            let data = serde_json::json!({"imported": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-import", data).with_entity(id.clone()),
            )
        }
    };

    adapter.shutdown().await?;
    result
}

fn load_profile(path: &Path) -> CliResult<wf_types::llm::LlmProfile> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::Arguments(format!("invalid profile JSON in {}: {e}", path.display()))
    })
}
