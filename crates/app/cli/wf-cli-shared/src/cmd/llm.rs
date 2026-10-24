use std::io::IsTerminal;
use std::path::Path;

use wf_client::WfClient;

use crate::args::{Cli, LlmAuthSub, LlmProfileSub, LlmProviderSub, LlmTemplateSub};
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
            let data: serde_json::Value = client
                .get_json(&format!("/api/v1/llm/profiles/{id}"))
                .await?;
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
            client
                .post_json::<_, serde_json::Value>("/api/v1/llm/profiles", &profile)
                .await?;
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

/// Run an `llm auth` subcommand: login (drives the host session state
/// machine over HTTP), logout, or status.
pub async fn run_auth(cli: &Cli, sub: &LlmAuthSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();

    match sub {
        LlmAuthSub::Login { provider } => match provider {
            Some(provider_id) => drive_login(cli, client, provider_id).await,
            None => {
                let data: serde_json::Value = client.get_json("/api/v1/llm/auth").await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-auth-login-options", data),
                )
            }
        },
        LlmAuthSub::Logout { provider } => {
            client
                .post_json::<_, serde_json::Value>(
                    "/api/v1/llm/auth/logout",
                    &serde_json::json!({"provider_id": provider}),
                )
                .await?;
            let data = serde_json::json!({"logged_out": provider});
            render_envelope(
                cli.output,
                OutputEnvelope::success("llm-auth-logout", data).with_entity(provider.clone()),
            )
        }
        LlmAuthSub::Status => {
            let data: serde_json::Value = client.get_json("/api/v1/llm/auth").await?;
            render_envelope(cli.output, OutputEnvelope::success("llm-auth-status", data))
        }
    }
}

/// Default seconds between login polls when the server does not advertise
/// an interval (RFC 8628 default).
const LOGIN_POLL_FALLBACK_SECS: u64 = 5;

/// Drive one login session to completion: render challenge events, poll
/// while the flow waits on an external event, and submit collected input.
/// Text mode renders human-readable progress on stderr; JSON mode emits
/// every state verbatim on stdout for headless callers.
async fn drive_login(cli: &Cli, client: &WfClient, provider_id: &str) -> CliResult<()> {
    let started: serde_json::Value = client
        .post_json(
            "/api/v1/llm/auth/login",
            &serde_json::json!({"provider_id": provider_id}),
        )
        .await?;
    let session_id = started
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CliError::Configuration("login response missing session_id".to_string()))?
        .to_string();
    let mut state = started
        .get("state")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let mut poll_secs = LOGIN_POLL_FALLBACK_SECS;

    loop {
        emit_state(cli, &state);
        let kind = state
            .as_object()
            .and_then(|obj| obj.keys().next().cloned())
            .unwrap_or_default();
        match kind.as_str() {
            "Completed" => {
                let data = serde_json::json!({
                    "provider": provider_id,
                    "session_id": session_id,
                    "state": state,
                });
                return render_envelope(
                    cli.output,
                    OutputEnvelope::success("llm-auth-login", data).with_entity(provider_id),
                );
            }
            "Failed" => {
                let error = state
                    .get("Failed")
                    .and_then(|f| f.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("login failed")
                    .to_string();
                return Err(CliError::Business(format!(
                    "login for '{provider_id}' failed: {error}"
                )));
            }
            "AwaitingInput" => {
                let prompt = state
                    .get("AwaitingInput")
                    .and_then(|a| a.get("prompt"))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let input = collect_prompt(&prompt)?;
                state = client
                    .post_json(
                        &format!("/api/v1/llm/auth/login/{session_id}/submit"),
                        &serde_json::json!({"input": input}),
                    )
                    .await?;
            }
            // Challenge and Pending both wait on an external event; poll.
            _ => {
                if let Some(secs) = device_interval_secs(&state) {
                    poll_secs = secs;
                }
                tokio::time::sleep(std::time::Duration::from_secs(poll_secs)).await;
                state = client
                    .get_json(&format!("/api/v1/llm/auth/login/{session_id}"))
                    .await?;
            }
        }
    }
}

/// Emit one login state: raw JSON in JSON mode, rendered events on stderr
/// in text mode (stdout stays reserved for the final envelope).
fn emit_state(cli: &Cli, state: &serde_json::Value) {
    if cli.output == crate::output::OutputFormat::Json {
        println!("{}", serde_json::to_string(state).unwrap_or_default());
        return;
    }
    let Some(events) = state
        .get("Challenge")
        .or_else(|| state.get("Pending"))
        .or_else(|| state.get("AwaitingInput"))
        .or_else(|| state.get("Failed"))
        .and_then(|s| s.get("events"))
        .and_then(|e| e.as_array())
    else {
        return;
    };
    for event in events {
        let Some(kind) = event.as_object().and_then(|obj| obj.keys().next().cloned()) else {
            continue;
        };
        let body = event.get(&kind).cloned().unwrap_or(serde_json::Value::Null);
        match kind.as_str() {
            "AuthUrl" => {
                if let Some(url) = body.get("url").and_then(|v| v.as_str()) {
                    eprintln!("open this URL to authorize: {url}");
                }
                if let Some(hint) = body.get("instructions").and_then(|v| v.as_str()) {
                    eprintln!("{hint}");
                }
            }
            "DeviceCode" => {
                if let Some(uri) = body.get("verification_uri").and_then(|v| v.as_str()) {
                    eprintln!("visit: {uri}");
                }
                if let Some(code) = body.get("user_code").and_then(|v| v.as_str()) {
                    eprintln!("enter code: {code}");
                }
            }
            "Info" | "Progress" => {
                if let Some(message) = body.get("message").and_then(|v| v.as_str()) {
                    eprintln!("{message}");
                }
            }
            _ => {}
        }
    }
}

/// The device-flow poll interval advertised by the latest DeviceCode event.
fn device_interval_secs(state: &serde_json::Value) -> Option<u64> {
    let events = state
        .get("Challenge")
        .or_else(|| state.get("Pending"))
        .or_else(|| state.get("AwaitingInput"))
        .and_then(|s| s.get("events"))
        .and_then(|e| e.as_array())?;
    events.iter().rev().find_map(|event| {
        let body = event.get("DeviceCode")?;
        body.get("interval_seconds")?.as_u64()
    })
}

/// Read the answer to one prompt from stdin. Login is interactive by
/// nature: without a TTY the caller is told to use ambient keys instead
/// (headless contract).
fn collect_prompt(prompt: &serde_json::Value) -> CliResult<String> {
    if !std::io::stdin().is_terminal() {
        return Err(CliError::Configuration(
            "login needs interactive input but stdin is not a terminal; \
             configure an ambient API key (env var) for non-interactive hosts"
                .to_string(),
        ));
    }
    let kind = prompt
        .as_object()
        .and_then(|obj| obj.keys().next().cloned())
        .unwrap_or_else(|| "Text".to_string());
    let body = prompt
        .get(&kind)
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let message = body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("input");
    match kind.as_str() {
        "Select" => {
            let options = body
                .get("options")
                .and_then(|o| o.as_array())
                .cloned()
                .unwrap_or_default();
            for (index, option) in options.iter().enumerate() {
                let label = option
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("option");
                eprintln!("  [{}] {label}", index + 1);
            }
            eprint!("? {message} (number) ");
            let _ = std::io::Write::flush(&mut std::io::stderr());
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(CliError::Io)?;
            let choice: usize = input
                .trim()
                .parse()
                .map_err(|_| CliError::Arguments("invalid selection".to_string()))?;
            let option = options
                .get(choice.saturating_sub(1))
                .ok_or_else(|| CliError::Arguments("selection out of range".to_string()))?;
            option
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| CliError::Arguments("selected option has no id".to_string()))
        }
        _ => {
            eprint!("? {message} ");
            let _ = std::io::Write::flush(&mut std::io::stderr());
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(CliError::Io)?;
            Ok(input.trim_end_matches(['\n', '\r']).to_string())
        }
    }
}
