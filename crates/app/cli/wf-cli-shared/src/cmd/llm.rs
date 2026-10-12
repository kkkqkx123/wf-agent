use std::path::Path;

use crate::args::{Cli, LlmProfileSub, LlmProviderSub, LlmTemplateSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &LlmProfileSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();

    match sub {
        LlmProfileSub::List => {
            let data: serde_json::Value = client.get_json("/api/v1/llm/profiles").await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-list", data),
            )
        }
        LlmProfileSub::Show { id } => {
            let data: serde_json::Value =
                client.get_json(&format!("/api/v1/llm/profiles/{id}")).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-show", data).with_entity(id.clone()),
            )
        }
        LlmProfileSub::Create { file } => {
            let profile = load_profile_json(Path::new(file))?;
            let id = profile
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            client.post_json::<_, serde_json::Value>("/api/v1/llm/profiles", &profile).await?;
            let data = serde_json::json!({"id": id, "created": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-create", data).with_entity(id),
            )
        }
        LlmProfileSub::Update { id, file } => {
            let mut profile = load_profile_json(Path::new(file))?;
            if let Some(obj) = profile.as_object_mut() {
                obj.insert("id".to_string(), id.clone().into());
            }
            let data: serde_json::Value = client
                .put_json(&format!("/api/v1/llm/profiles/{id}"), &profile)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-update", data).with_entity(id.clone()),
            )
        }
        LlmProfileSub::Delete { id } => {
            client
                .delete_json::<serde_json::Value>(&format!("/api/v1/llm/profiles/{id}"))
                .await?;
            let data = serde_json::json!({"deleted": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-delete", data).with_entity(id.clone()),
            )
        }
        LlmProfileSub::Validate { file } => {
            let profile = load_profile_json(Path::new(file))?;
            let data: serde_json::Value = client
                .post_json("/api/v1/llm/profiles/validate", &profile)
                .await?;
            let envelope = OutputEnvelope::success("llm-profile-validate", data);
            render_envelope(cli.output, envelope)
        }
        LlmProfileSub::Default { set } => {
            if let Some(id) = set {
                client
                    .post_json::<_, serde_json::Value>(
                        &format!("/api/v1/llm/profiles/{id}/default"),
                        &serde_json::json!({}),
                    )
                    .await?;
                let data = serde_json::json!({"default": id});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-default-set", data)
                        .with_entity(id.clone()),
                )
            } else {
                let data: serde_json::Value =
                    client.get_json("/api/v1/llm/profiles/default").await?;
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
                let templates: serde_json::Value =
                    client.get_json("/api/v1/llm/profile-templates").await?;
                let items = templates.as_array().cloned().unwrap_or_default();
                let filtered: Vec<_> = items
                    .into_iter()
                    .filter(|t| {
                        let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let description =
                            t.get("description").and_then(|v| v.as_str()).unwrap_or("");
                        if let Some(k) = kind {
                            let k_lower = k.to_lowercase();
                            let format = t
                                .get("profile")
                                .and_then(|p| p.get("format"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !name.to_lowercase().contains(&k_lower)
                                && !description.to_lowercase().contains(&k_lower)
                                && !format.to_lowercase().contains(&k_lower)
                            {
                                return false;
                            }
                        }
                        if let Some(cat) = category {
                            if !description.to_lowercase().contains(&cat.to_lowercase())
                                && !name.to_lowercase().contains(&cat.to_lowercase())
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
                                let hay = format!("{name} {description}").to_lowercase();
                                if !wanted.iter().any(|w| hay.contains(w)) {
                                    return false;
                                }
                            }
                        }
                        if let Some(a) = author {
                            if !description.to_lowercase().contains(&a.to_lowercase())
                                && !name.to_lowercase().contains(&a.to_lowercase())
                            {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();
                let data = serde_json::Value::Array(filtered);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-profile-template-list", data),
                )
            }
        },
        LlmProfileSub::Export { id, file } => {
            let value: serde_json::Value = client
                .get_json(&format!("/api/v1/llm/profiles/{id}/export"))
                .await?;
            if cli.output == crate::output::OutputFormat::Text {
                eprintln!("warning: api_key masked as ***HIDDEN***");
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
            let body = serde_json::json!({"json": content});
            let id: String = client
                .post_json("/api/v1/llm/profiles/import", &body)
                .await?;
            let data = serde_json::json!({"imported": id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-profile-import", data).with_entity(id),
            )
        }
    }
}

fn load_profile_json(path: &Path) -> CliResult<serde_json::Value> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CliError::Arguments(format!("invalid profile JSON in {}: {e}", path.display()))
    })
}

/// Run an `llm-provider` subcommand: list definitions, show one
/// definition, or list its discovered models.
pub async fn run_provider(cli: &Cli, sub: &LlmProviderSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();

    match sub {
        LlmProviderSub::List => {
            let data: serde_json::Value = client.get_json("/api/v1/llm/providers").await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-provider-list", data),
            )
        }
        LlmProviderSub::Show { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/llm/providers/{id}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-provider-show", data).with_entity(id.clone()),
            )
        }
        LlmProviderSub::Models { id } => {
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/llm/providers/{id}/models"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-provider-models", data).with_entity(id.clone()),
            )
        }
    }
}
