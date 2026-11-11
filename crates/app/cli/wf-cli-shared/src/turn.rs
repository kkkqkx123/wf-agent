//! Turn parameters and server request bodies shared by the CLI forms.
//!
//! A turn executes either an agent message or a workflow. This module owns
//! the UI-facing shape plus its translation into the `wf-server` wire bodies;
//! (`RunAgentLoopBody` / `ExecuteBody`); the server re-runs template
//! resolution and defaults, so the client only fills what the flags provide.

use serde_json::Value;

/// Built-in agent definition id used when no `--agent` is given. Mirrors the
/// server default for empty ids.
pub const DEFAULT_AGENT: &str = "@standard/main";

/// Model profile id used when no `--model` is given.
pub const DEFAULT_MODEL: &str = "default";

/// What a turn executes.
#[derive(Debug, Clone)]
pub enum TurnKind {
    Agent {
        prompt: String,
        /// Structured image payload of the turn.
        images: Vec<wf_types::message::UserImageInput>,
        /// Structured text attachments of the turn.
        attachments: Vec<wf_types::message::UserTextAttachment>,
    },
    Workflow {
        workflow_id: String,
        input: Option<Value>,
    },
}

/// Parameters for a turn, shared between headless and mini forms.
#[derive(Debug, Clone)]
pub struct TurnParams {
    pub agent: Option<String>,
    pub model: Option<String>,
    pub approve_prefixes: Vec<String>,
    /// Server-side run budget in milliseconds (`--timeout`).
    pub timeout_ms: Option<u64>,
    /// Structured agent context (`--context` / `--context-file`).
    /// The server validates it against the agent `context_schema` when the
    /// template declares one; empty maps skip that gate.
    pub context: serde_json::Map<String, Value>,
    /// Prior conversation seeding the run. Headless single-shot turns leave
    /// this empty; the mini session fills it from its transcript.
    pub conversation: Vec<wf_types::message::Message>,
    pub kind: TurnKind,
}

impl TurnParams {
    pub fn prompt(&self) -> Option<&str> {
        match &self.kind {
            TurnKind::Agent { prompt, .. } => Some(prompt.as_str()),
            TurnKind::Workflow { .. } => None,
        }
    }

    /// Structured image payload of the turn; empty for workflow turns.
    pub fn images(&self) -> &[wf_types::message::UserImageInput] {
        match &self.kind {
            TurnKind::Agent { images, .. } => images,
            TurnKind::Workflow { .. } => &[],
        }
    }

    /// Structured text attachments of the turn; empty for workflow turns.
    pub fn attachments(&self) -> &[wf_types::message::UserTextAttachment] {
        match &self.kind {
            TurnKind::Agent { attachments, .. } => attachments,
            TurnKind::Workflow { .. } => &[],
        }
    }

    /// Translate into the typed `POST /api/v1/agent-loops/{id}/run|stream` body.
    /// Single source for both the SSE stream (serialized to JSON) and the
    /// blocking fallback (passed to the generated client directly), so field
    /// names cannot drift between the two paths.
    pub fn agent_typed_body(&self) -> wf_client::models::RunAgentLoopBody {
        let prompt = self.prompt().unwrap_or_default();
        // Client passes raw text through; the server is the single washing
        // point and the single fallback owner for empty agent and model.
        let conversation: Vec<serde_json::Value> = self
            .conversation
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
            .collect();
        let images: Vec<serde_json::Value> = self
            .images()
            .iter()
            .map(|image| serde_json::to_value(image).unwrap_or(Value::Null))
            .collect();
        let attachments: Vec<serde_json::Value> = self
            .attachments()
            .iter()
            .map(|attachment| serde_json::to_value(attachment).unwrap_or(Value::Null))
            .collect();
        wf_client::models::RunAgentLoopBody {
            agent_id: self.agent.clone(),
            model: self.model.clone().unwrap_or_default(),
            message: prompt.to_string(),
            images: Some(images),
            attachments: Some(attachments),
            context: self.context.clone(),
            conversation: Some(conversation),
            max_execution_time: self.timeout_ms.map(|v| v as i64),
            max_iterations: None,
            tool_call_protocol: Value::Null,
            available_tool_names: None,
            checkpoint_message_interval: None,
            context_schema: None,
            discoverable_tool_names: None,
            enable_general_tool: None,
            enable_token_tracking: None,
            file_scope: None,
            hidden_tool_names: None,
            hooks: None,
            initial_tool_names: None,
            token_limit: None,
            token_warning_threshold: None,
            temporary_mode: None,
        }
    }

    /// Translate into the `POST /api/v1/agent-loops/{id}/run|stream` body.
    /// The server defaults empty ids and empty models to the built-in agent
    /// and model template, so `None` stays empty rather than baking defaults
    /// in here. Iteration budget is intentionally omitted: the server applies its default when
    /// neither the request nor the template sets one, so a hardcoded value
    /// here would override template configuration.
    pub fn agent_request_body(&self) -> Value {
        serde_json::to_value(self.agent_typed_body()).unwrap_or(Value::Null)
    }

    /// Typed `POST /api/v1/workflows/{id}/execute*` body.
    pub fn workflow_typed_body(input: Option<&Value>) -> wf_client::models::ExecuteBody {
        wf_client::models::ExecuteBody {
            input: input.cloned(),
            file_scope: None,
            version: None,
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: None,
            node_timeout: None,
            max_pause_duration: None,
            max_navigation_multiplier: None,
            loop_max_iterations_cap: None,
            debug: None,
        }
    }

    /// Translate into the `POST /api/v1/workflows/{id}/execute*` body.
    pub fn workflow_request_body(input: Option<&Value>) -> Value {
        serde_json::to_value(Self::workflow_typed_body(input)).unwrap_or(Value::Null)
    }
}

/// Validate an already-parsed workflow input value against the shared
/// shape rules (W1-W3): null/missing is no input, an object passes
/// through, anything else (array, string, number, bool) is a hard error.
pub fn validate_workflow_input_value(input: Option<&Value>) -> Result<(), String> {
    wf_types::workflow_execution::validate_workflow_input_shape(input)
        .map_err(|reason| format!("invalid --input: {reason}"))
}

fn input_kind(value: &Value) -> &'static str {
    if value.is_array() {
        "array"
    } else if value.is_string() {
        "string"
    } else if value.is_number() {
        "number"
    } else {
        "boolean"
    }
}

/// Parse the structured agent context from `--context` (inline JSON
/// object) and `--context-file` (JSON file holding an object). File
/// content loads first so inline keys win on conflict; both must be JSON
/// objects when present. Failures are hard errors: context silently
/// degrading to empty would hide a typo from the user.
pub fn parse_agent_context(
    inline: Option<&str>,
    file: Option<&std::path::Path>,
) -> Result<serde_json::Map<String, Value>, String> {
    let mut merged = serde_json::Map::new();
    if let Some(path) = file {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read --context-file '{}': {e}", path.display()))?;
        match serde_json::from_str::<Value>(&text)
            .map_err(|e| format!("invalid --context-file JSON: {e}"))?
        {
            Value::Object(map) => merged.extend(map),
            other => {
                return Err(format!(
                    "invalid --context-file: must be a JSON object, got {}",
                    input_kind(&other),
                ));
            }
        }
    }
    match inline {
        None => Ok(merged),
        Some(s) if s.trim().is_empty() => Ok(merged),
        Some(s) => {
            match serde_json::from_str::<Value>(s)
                .map_err(|e| format!("invalid --context JSON: {e}"))?
            {
                Value::Object(map) => {
                    merged.extend(map);
                    Ok(merged)
                }
                Value::Null => Ok(merged),
                other => Err(format!(
                    "invalid --context: must be a JSON object, got {}",
                    input_kind(&other),
                )),
            }
        }
    }
}

/// Parse a loose scalar value: JSON when possible, plain string otherwise.
/// Shared by workflow `--var` values and per-key context edits so both
/// surfaces keep the same ergonomic shape (`--var n=42` stays numeric
/// while `--var city=paris` stays a string).
pub fn parse_loose_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    serde_json::from_str(trimmed).unwrap_or(Value::String(trimmed.to_string()))
}

/// Parse one `--var KEY=VALUE` assignment. The key must be non-empty;
/// the value follows [`parse_loose_value`].
pub fn parse_workflow_var_arg(arg: &str) -> Result<(String, Value), String> {
    let (key, raw) = arg
        .split_once('=')
        .ok_or_else(|| format!("invalid --var '{arg}': expected KEY=VALUE"))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("invalid --var '{arg}': key must not be empty"));
    }
    Ok((key.to_string(), parse_loose_value(raw)))
}

/// Parse one per-key context assignment for the TUI context commands.
/// The key follows the same template path rule the server enforces on
/// agent input, so typos fail locally with the reason instead of costing
/// a server round trip. The value follows [`parse_loose_value`].
pub fn parse_context_key_value(key: &str, raw_value: &str) -> Result<(String, Value), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("context key must not be empty".to_string());
    }
    if let Some(reason) = wf_types::template::validate_template_path(trimmed) {
        return Err(format!("invalid context key '{trimmed}': {reason}"));
    }
    Ok((trimmed.to_string(), parse_loose_value(raw_value)))
}

/// Parse every `--var` assignment into a top-level map. Duplicate keys
/// resolve last-wins so repeated flags behave like repeated query params.
pub fn parse_workflow_var_args(args: &[String]) -> Result<serde_json::Map<String, Value>, String> {
    let mut map = serde_json::Map::new();
    for arg in args {
        let (key, value) = parse_workflow_var_arg(arg)?;
        map.insert(key, value);
    }
    Ok(map)
}

/// Load workflow input from `--input-file`. Blank files count as no input;
/// non-object JSON is a hard error matching the inline `--input` rules.
pub fn load_workflow_input_file(path: Option<&std::path::Path>) -> Result<Option<Value>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read --input-file '{}': {e}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid --input-file JSON: {e}"))?;
    validate_workflow_input_value(Some(&value))?;
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// Merge the three workflow input channels with `file < inline < vars`
/// priority at the top level. All three are objects when present; the
/// merged object keeps the shared shape contract (object or empty).
pub fn merge_workflow_inputs(
    file: Option<Value>,
    inline: Option<Value>,
    vars: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let mut merged = serde_json::Map::new();
    for source in [file, inline] {
        if let Some(Value::Object(map)) = source {
            merged.extend(map);
        }
    }
    merged.extend(vars.clone());
    if merged.is_empty() {
        None
    } else {
        Some(Value::Object(merged))
    }
}

/// Resolve the three workflow input channels into one merged value.
/// Parse failures in any channel are hard errors; nothing degrades to
/// empty silently.
pub fn resolve_workflow_input(
    inline: Option<&str>,
    file: Option<&std::path::Path>,
    vars: &[String],
) -> Result<Option<Value>, String> {
    let file_value = load_workflow_input_file(file)?;
    let inline_value = parse_workflow_input(inline)?;
    let var_map = parse_workflow_var_args(vars)?;
    Ok(merge_workflow_inputs(file_value, inline_value, &var_map))
}

/// Local pre-check of merged workflow input against the declared
/// variables fetched from the server. Mirrors the server gate
/// (backfill defaults, then type plus required) and reports every issue
/// together so one round fixes all keys.
pub fn prevalidate_workflow_input_locally(
    input: Option<&Value>,
    variables: Option<&[wf_types::workflow_execution::VariableDefinition]>,
) -> Result<(), String> {
    prevalidate_workflow_input_locally_with_policy(input, variables, false)
}

/// Policy variant of [`prevalidate_workflow_input_locally`]. `strict`
/// rejects unknown top-level keys; the definition side owns the switch.
pub fn prevalidate_workflow_input_locally_with_policy(
    input: Option<&Value>,
    variables: Option<&[wf_types::workflow_execution::VariableDefinition]>,
    strict: bool,
) -> Result<(), String> {
    let issues = wf_types::workflow_execution::collect_workflow_input_issues_with_policy(
        input, variables, strict,
    );
    if issues.is_empty() {
        return Ok(());
    }
    Err(format!(
        "invalid workflow input ({} error(s)): {}",
        issues.len(),
        issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

/// Whether a free-form image spec names a remote payload rather than a
/// local file. Remote specs travel verbatim; anything else is read from disk.
pub fn is_remote_image_spec(spec: &str) -> bool {
    let trimmed = spec.trim();
    trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("data:")
}

/// Parse an optional `--image-detail` value. Empty means provider default.
pub fn parse_image_detail_arg(
    raw: Option<&str>,
) -> Result<Option<wf_types::message::ImageDetail>, String> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(value) => wf_types::message::parse_image_detail(value).map(Some),
    }
}

/// Load mixed local-path and remote-URL image specs with one detail hint.
/// Remote specs travel verbatim; local paths are read and inlined. The
/// combined stage is validated so oversized or mistyped stages fail here.
pub fn load_user_image_specs(
    specs: &[String],
    detail: Option<wf_types::message::ImageDetail>,
) -> Result<Vec<wf_types::message::UserImageInput>, String> {
    use base64::Engine as _;
    let mut images = Vec::with_capacity(specs.len());
    for spec in specs {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            return Err("cannot attach an empty image spec".to_string());
        }
        if is_remote_image_spec(trimmed) {
            let mut input = wf_types::message::UserImageInput::remote(trimmed.to_string());
            input.detail = detail;
            images.push(input);
            continue;
        }
        let path = std::path::PathBuf::from(trimmed);
        let Some(media) = wf_types::message::media_type_for_image_path(&path.to_string_lossy())
        else {
            return Err(format!(
                "cannot attach '{}': not a served image file (png, jpg, gif, webp) or remote https/http/data URL",
                path.display()
            ));
        };
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("cannot read image '{}': {e}", path.display()))?;
        if bytes.len() > wf_types::message::MAX_USER_IMAGE_BYTES {
            return Err(format!(
                "cannot attach '{}': {} bytes exceed the per-image limit of {} bytes",
                path.display(),
                bytes.len(),
                wf_types::message::MAX_USER_IMAGE_BYTES
            ));
        }
        let mut input = wf_types::message::UserImageInput::inline(
            base64::engine::general_purpose::STANDARD.encode(&bytes),
            media,
        );
        input.detail = detail;
        images.push(input);
    }
    wf_types::message::validate_user_images(&images)
        .map_err(|reason| format!("invalid image payload: {reason}"))?;
    Ok(images)
}

/// Load `--image` paths plus `--image-url` remotes with one detail hint.
/// Both channels merge into one validated stage so headless and
/// interactive forms share a single limit check.
pub fn load_all_user_images(
    paths: &[std::path::PathBuf],
    urls: &[String],
    detail: Option<wf_types::message::ImageDetail>,
) -> Result<Vec<wf_types::message::UserImageInput>, String> {
    let mut specs: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    specs.extend(urls.iter().cloned());
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    load_user_image_specs(&specs, detail)
}

/// Load `--attach` text files into structured attachments. Files read as
/// UTF-8 text; NUL bytes are rejected as binary. Content beyond the
/// per-file limit is truncated with an explicit marker and original size,
/// mirroring the web composer staging rule. The shared validator owns the
/// final count and budget gate so headless and interactive forms agree.
pub fn load_user_attachment_specs(
    paths: &[std::path::PathBuf],
) -> Result<Vec<wf_types::message::UserTextAttachment>, String> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("cannot read attachment '{}': {e}", path.display()))?;
        if bytes.contains(&0) {
            return Err(format!(
                "cannot attach '{}': looks like a binary file; send images via --image instead",
                path.display()
            ));
        }
        let text = String::from_utf8(bytes).map_err(|_| {
            format!(
                "cannot attach '{}': not valid UTF-8 text; send images via --image instead",
                path.display()
            )
        })?;
        let chars = text.chars().count();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        if chars > wf_types::message::MAX_TEXT_ATTACHMENT_CHARS {
            let truncated: String = text
                .chars()
                .take(wf_types::message::MAX_TEXT_ATTACHMENT_CHARS)
                .collect();
            out.push(wf_types::message::UserTextAttachment {
                name,
                content: truncated,
                truncated: true,
                original_chars: Some(chars),
            });
        } else {
            out.push(wf_types::message::UserTextAttachment {
                name,
                content: text,
                truncated: false,
                original_chars: None,
            });
        }
    }
    wf_types::message::validate_user_attachments(&out)
        .map_err(|reason| format!("invalid attachment payload: {reason}"))?;
    Ok(out)
}

/// Short display summary of one staged attachment for diagnostics.
pub fn describe_user_attachment(attachment: &wf_types::message::UserTextAttachment) -> String {
    let chars = attachment.content.chars().count();
    if attachment.truncated {
        format!(
            "{} ({} chars, truncated from {})",
            attachment.name,
            chars,
            attachment.original_chars.unwrap_or(chars)
        )
    } else {
        format!("{} ({} chars)", attachment.name, chars)
    }
}

/// Short display summary of one staged image for diagnostics: remote
/// addresses show the address, inline payloads show the decoded size.
pub fn describe_user_image(image: &wf_types::message::UserImageInput) -> String {
    if let Some(url) = image
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return url.to_string();
    }
    match wf_types::message::user_image_bytes(image) {
        Some(bytes) => format!("{bytes} bytes inline"),
        None => "empty payload".to_string(),
    }
}

/// One-line stage budget for the interactive forms: staged counts against
/// the per-turn limits plus decoded payload totals. Remote images carry
/// unknown sizes, so they count toward the image count only.
pub fn stage_budget_line(
    images: &[wf_types::message::UserImageInput],
    attachments: &[wf_types::message::UserTextAttachment],
) -> String {
    let inline_bytes: usize = images
        .iter()
        .filter_map(wf_types::message::user_image_bytes)
        .sum();
    let remote = images
        .iter()
        .filter(|image| {
            image
                .url
                .as_deref()
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .is_some()
        })
        .count();
    let chars: usize = attachments
        .iter()
        .map(|attachment| attachment.content.chars().count())
        .sum();
    format!(
        "budget: {}/{} images ({} of {} bytes{}), {}/{} files ({} of {} chars)",
        images.len(),
        wf_types::message::MAX_USER_IMAGES_PER_TURN,
        inline_bytes,
        wf_types::message::MAX_USER_IMAGES_TOTAL_BYTES,
        if remote > 0 {
            format!(", {remote} remote")
        } else {
            String::new()
        },
        attachments.len(),
        wf_types::message::MAX_TEXT_ATTACHMENTS_PER_TURN,
        chars,
        wf_types::message::MAX_TEXT_ATTACHMENTS_TOTAL_CHARS,
    )
}

/// Helper to parse workflow input JSON string.
pub fn parse_workflow_input(input: Option<&str>) -> Result<Option<Value>, String> {
    match input {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => {
            let value: Value =
                serde_json::from_str(s).map_err(|e| format!("invalid --input JSON: {e}"))?;
            validate_workflow_input_value(Some(&value))?;
            if value.is_null() {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        }
    }
}

#[cfg(test)]
mod test;
