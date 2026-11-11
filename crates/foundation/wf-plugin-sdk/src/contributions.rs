//! Plugin-author contribution contracts: payload types and handler traits.
//!
//! These traits are what a plugin implements to contribute behavior
//! (nodes/tools/LLM/middleware) to the host. Registration-side machinery
//! (`ContributionManager`, `ContributionRegistrar`) stays in the host crate
//! `wf-plugin`.

use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::PluginResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExecutionContext {
    pub node_id: String,
    pub inputs: Value,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginNodeResult {
    pub outputs: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolContext {
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolResult {
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEventData {
    pub event_type: String,
    pub data: Value,
}

/// Next function type for middleware chains.
///
/// The caller passes the (possibly rewritten) context downstream; the
/// returned future resolves to the final context after the rest of the
/// chain (and any response-side rewrites on the way back up).
pub type NextFn = Box<dyn FnOnce(Value) -> BoxFuture<'static, PluginResult<Value>> + Send>;

// ── CLI contributions (declarative, host-rendered) ──────────────────────
// A CLI contribution describes interface surface as data: the host owns
// every widget, key binding and rendering decision. Sandboxed backends
// (Wasm, Lua) may only send these descriptions; executable UI stays on
// the native backend behind an explicit dispatch channel. The host
// validates each description and falls back to built-in behavior on any
// failure, so a bad contribution never breaks the session.
/// Applicability condition for a CLI command. Every listed screen or role
/// admits the command; empty lists admit everywhere. The host evaluates
/// the conjunction; unknown future fields must be ignored, never assumed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PluginCliCondition {
    /// Screen names admitting the command (empty means every screen).
    #[serde(default)]
    pub screens: Vec<String>,
    /// Identity roles admitting the command (`owner`, `guest`; empty
    /// means every role).
    #[serde(default)]
    pub roles: Vec<String>,
    /// Only offer the command while connected to the server.
    #[serde(default)]
    pub require_connected: bool,
    /// Hide the command while any overlay or modal is open.
    #[serde(default)]
    pub hide_when_overlay: bool,
}

/// One user-facing command contributed by a plugin: slash entry, palette
/// row and help line share this definition. The plugin id owns the
/// namespace: two plugins registering one id is a conflict, never an
/// override. Execution dispatch stays server-side; the CLI resolves the
/// name and routes arguments through the host command protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginCliCommand {
    /// Stable command id (`<plugin>.<verb>`); doubles as the registry key.
    pub id: String,
    /// Short display title for palette rows and help.
    pub title: String,
    /// Palette group and help section.
    pub group: String,
    /// One-line description for palette filtering and help.
    pub description: String,
    /// Slash name without the leading slash (`None` hides the slash entry
    /// while keeping the palette row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slash_name: Option<String>,
    /// Extra slash spellings, all without the leading slash.
    #[serde(default)]
    pub slash_aliases: Vec<String>,
    /// Argument syntax shown in help (`""` when the command takes none).
    #[serde(default)]
    pub slash_argument: String,
    /// Whether the command appears in the command palette.
    #[serde(default = "default_palette")]
    pub palette: bool,
    /// Display-only default key hint (`"ctrl+k"`); the host keymap wins
    /// and launcher-owned keys are never granted through this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
    /// Applicability condition evaluated by the host.
    #[serde(default)]
    pub when: PluginCliCondition,
}

fn default_palette() -> bool {
    true
}

impl PluginCliCommand {
    /// Reject ids and slash spellings the host could never route: empty
    /// keys, whitespace, or a leading slash. Palette rows without a slash
    /// name are also rejected so every listed command stays invokable.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("cli-command id must not be empty".to_string());
        }
        if self.id.chars().any(char::is_whitespace) {
            return Err(format!(
                "cli-command id '{}' must not contain whitespace",
                self.id
            ));
        }
        if self.id.starts_with('/') {
            return Err(format!(
                "cli-command id '{}' must not start with '/'",
                self.id
            ));
        }
        if self.title.trim().is_empty() {
            return Err(format!("cli-command '{}' needs a title", self.id));
        }
        match &self.slash_name {
            None if self.palette => {
                return Err(format!(
                    "cli-command '{}' is in the palette but has no slash name",
                    self.id
                ));
            }
            Some(name) => check_slash_spelling(&self.id, name)?,
            None => {}
        }
        for alias in &self.slash_aliases {
            check_slash_spelling(&self.id, alias)?;
        }
        Ok(())
    }
}

fn check_slash_spelling(id: &str, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("cli-command '{id}' has an empty slash name"));
    }
    if name.starts_with('/') || name.chars().any(char::is_whitespace) {
        return Err(format!(
            "cli-command '{id}' slash name '{name}' must be a bare word"
        ));
    }
    Ok(())
}

/// One completion source contributed by a plugin: a set of trigger
/// characters plus the static items offered under them. The host tries
/// sources in registration order and shows the first hit; dynamic
/// providers re-register on change instead of holding callbacks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginCompletionItem {
    /// Text inserted on accept.
    pub value: String,
    /// Display label (defaults to the value in the host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// One-line description shown beside the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Static completion source; `id` is the registry key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginCompletionSource {
    pub id: String,
    /// Trigger characters activating this source (`#`, `@`, `/`).
    pub trigger_chars: Vec<String>,
    /// Items offered while the trigger is active.
    #[serde(default)]
    pub items: Vec<PluginCompletionItem>,
}

impl PluginCompletionSource {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("completion-provider id must not be empty".to_string());
        }
        if self.trigger_chars.is_empty() {
            return Err(format!(
                "completion-provider '{}' needs at least one trigger character",
                self.id
            ));
        }
        for trigger in &self.trigger_chars {
            if trigger.is_empty() || trigger.chars().any(char::is_whitespace) {
                return Err(format!(
                    "completion-provider '{}' has an invalid trigger",
                    self.id
                ));
            }
        }
        for item in &self.items {
            if item.value.is_empty() {
                return Err(format!(
                    "completion-provider '{}' has an item with an empty value",
                    self.id
                ));
            }
        }
        Ok(())
    }
}

/// One settings row contributed by a plugin. Values live under the
/// visitor's `plugins.<plugin>.<key>` namespace; changing one redraws
/// every settings card through the host, never through plugin code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginSettingField {
    /// Dotted key (`<plugin>.<name>`); doubles as the registry key.
    pub key: String,
    /// Display label for the settings row.
    pub label: String,
    /// Allowed values; empty means free-form text.
    #[serde(default)]
    pub values: Vec<String>,
    /// Value used when nothing is stored.
    pub fallback: String,
}

impl PluginSettingField {
    pub fn validate(&self) -> Result<(), String> {
        if !self.key.contains('.') || self.key.trim().is_empty() {
            return Err(format!(
                "setting-field key '{}' must look like '<plugin>.<name>'",
                self.key
            ));
        }
        if self.key.chars().any(char::is_whitespace) {
            return Err(format!(
                "setting-field key '{}' must not contain whitespace",
                self.key
            ));
        }
        if !self.values.is_empty() && !self.values.contains(&self.fallback) {
            return Err(format!(
                "setting-field '{}' fallback is not one of its values",
                self.key
            ));
        }
        Ok(())
    }
}

/// Declarative tool-call and tool-result summaries for one tool. The host
/// substitutes `{args}` (compact argument text) and `{result}` (truncated
/// result text) into the templates and renders the outcome with the
/// theme; unknown placeholders stay verbatim so typos stay visible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginToolSummary {
    /// Tool name this summary renders; doubles as the registry key.
    pub tool_name: String,
    /// Template for the pending/running call line.
    #[serde(default)]
    pub call_template: String,
    /// Template for the settled result line.
    #[serde(default)]
    pub result_template: String,
}

impl PluginToolSummary {
    pub fn validate(&self) -> Result<(), String> {
        if self.tool_name.trim().is_empty() {
            return Err("tool-summary tool name must not be empty".to_string());
        }
        if self.call_template.is_empty() && self.result_template.is_empty() {
            return Err(format!(
                "tool-summary '{}' needs at least one template",
                self.tool_name
            ));
        }
        Ok(())
    }
}

/// Render one tool summary template: `{args}` takes the compact argument
/// text, `{result}` the truncated result text. Display-only; the stored
/// tool payload never changes.
pub fn render_tool_summary(template: &str, args: &str, result: &str) -> String {
    template.replace("{args}", args).replace("{result}", result)
}

/// One literal Markdown rewrite applied to transcript surfaces before the
/// host renders. Rules run in registration order; each sees the previous
/// rule's output. Literal replacement only, so a rule can never diverge
/// into backtracking or capture-group surprises.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginMarkdownRule {
    /// Rule id; doubles as the registry key.
    pub id: String,
    /// Literal text to replace (never empty, never a pattern).
    pub pattern: String,
    /// Replacement text.
    #[serde(default)]
    pub replacement: String,
    /// Surfaces the rule runs on (`user`, `assistant`, `thinking`,
    /// `custom`, `summary`); empty means every surface.
    #[serde(default)]
    pub surfaces: Vec<String>,
}

impl PluginMarkdownRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("markdown-transformer id must not be empty".to_string());
        }
        if self.pattern.is_empty() {
            return Err(format!(
                "markdown-transformer '{}' needs a non-empty pattern",
                self.id
            ));
        }
        Ok(())
    }
}

/// Apply literal Markdown rules in order. Rules whose pattern is empty
/// are skipped so one bad entry cannot erase the whole document.
pub fn apply_markdown_rules(text: &str, rules: &[PluginMarkdownRule]) -> String {
    let mut out = text.to_string();
    for rule in rules {
        if rule.pattern.is_empty() {
            continue;
        }
        out = out.replace(rule.pattern.as_str(), rule.replacement.as_str());
    }
    out
}

/// Outcome of one middleware guest call: whether the chain continues and
/// which context travels onward.
#[derive(Debug, Clone, PartialEq)]
pub struct MiddlewareOutcome {
    pub proceed: bool,
    pub context: Value,
}

/// Parse a middleware guest response against the incoming context.
///
/// - JSON `true`/`false` selects proceed/skip with the context unchanged
///   (legacy boolean protocol).
/// - An object carrying `proceed` (default `true`) and/or `context`
///   (default: the incoming value) rewrites the request.
/// - Anything else keeps the legacy behavior: proceed with the incoming
///   context, so old guests are unaffected. Replacement is whole-value;
///   no merge-patch semantics are applied.
pub fn parse_middleware_outcome(response: &Value, incoming: &Value) -> MiddlewareOutcome {
    if let Some(proceed) = response.as_bool() {
        return MiddlewareOutcome {
            proceed,
            context: incoming.clone(),
        };
    }
    if let Some(object) = response.as_object() {
        if object.contains_key("proceed") || object.contains_key("context") {
            let proceed = object
                .get("proceed")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let context = object
                .get("context")
                .cloned()
                .unwrap_or_else(|| incoming.clone());
            return MiddlewareOutcome { proceed, context };
        }
    }
    MiddlewareOutcome {
        proceed: true,
        context: incoming.clone(),
    }
}

/// Outcome of one CLI input-chain handler: pass the text through,
/// rewrite it for the next handler, or take over with a direct echo that
/// skips the default dispatch path. Context travels as JSON
/// (`{"text": ..., "slash": ...}`); handlers only rewrite `text`.
#[derive(Debug, Clone, PartialEq)]
pub enum CliInputOutcome {
    /// Keep `text` unchanged and run the next handler.
    Pass,
    /// Replace the buffer text and run the next handler.
    Rewrite(String),
    /// Skip remaining handlers and the default path; `echo` is shown
    /// directly as the command result.
    Takeover(String),
}

/// Parse one CLI input-chain guest response against the incoming buffer
/// text. Shapes: `true` passes, `false` takes over with an empty echo
/// (legacy boolean protocol); `{"action": "pass"}` passes,
/// `{"action": "rewrite", "text": "..."}` rewrites,
/// `{"action": "takeover", "echo": "..."}` takes over. Anything else
/// passes with the incoming text so old guests never break dispatch.
pub fn parse_cli_input_outcome(response: &Value, incoming_text: &str) -> CliInputOutcome {
    if let Some(pass) = response.as_bool() {
        if pass {
            return CliInputOutcome::Pass;
        }
        return CliInputOutcome::Takeover(String::new());
    }
    if let Some(object) = response.as_object() {
        let action = object
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("pass");
        match action {
            "rewrite" => {
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    return CliInputOutcome::Rewrite(text.to_string());
                }
                return CliInputOutcome::Pass;
            }
            "takeover" => {
                let echo = object.get("echo").and_then(Value::as_str).unwrap_or("");
                return CliInputOutcome::Takeover(echo.to_string());
            }
            _ => return CliInputOutcome::Pass,
        }
    }
    let _ = incoming_text;
    CliInputOutcome::Pass
}

/// Apply one input-chain outcome to the running buffer text. Returns the
/// new text plus whether the chain took over (with its direct echo).
/// A handler failure must call this with `Pass` so dispatch falls back to
/// the built-in path instead of stalling the session.
pub fn apply_cli_input_outcome(text: &str, outcome: CliInputOutcome) -> (String, Option<String>) {
    match outcome {
        CliInputOutcome::Pass => (text.to_string(), None),
        CliInputOutcome::Rewrite(next) => (next, None),
        CliInputOutcome::Takeover(echo) => (text.to_string(), Some(echo)),
    }
}

#[async_trait]
pub trait PluginNodeHandler: Send + Sync {
    async fn execute(&self, ctx: PluginExecutionContext) -> PluginResult<PluginNodeResult>;
}

#[async_trait]
pub trait PluginToolExecutor: Send + Sync {
    async fn execute(&self, ctx: PluginToolContext) -> PluginResult<PluginToolResult>;
}

// ── low-level LLM codec ─────────────────────────────────────────────
// A codec contributes a full wire protocol format: request building,
// response and stream-chunk parsing, tool conversion and token counting.
// The contract is synchronous and JSON-based so Lua tables and native
// dispatch buffers share one shape. The host builds the actual HTTP
// request from the returned description and translates the returned
// values into its own LLM types; `parse_response` / `parse_stream_chunk`
// results must therefore follow the host `LlmResult` / stream-event JSON
// shapes documented by the runtime adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodecHttpRequest {
    /// HTTP method, e.g. `POST`.
    pub method: String,
    /// Fully joined URL, query parameters included.
    pub url: String,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    /// JSON body; `Value::Null` sends no body.
    #[serde(default)]
    pub body: Value,
}

pub trait PluginLlmCodec: Send + Sync {
    /// Describe the HTTP request for a structured LLM request and profile
    /// (both passed as JSON). The host constructs the request from it.
    fn build_request(&self, request: Value, profile: Value) -> PluginResult<CodecHttpRequest>;
    /// Parse a complete response body into the host `LlmResult` JSON shape.
    fn parse_response(&self, body: &str, request: Value) -> PluginResult<Value>;
    /// Parse one SSE data payload into the host stream-event JSON shape,
    /// or `None` for payloads carrying no event.
    fn parse_stream_chunk(&self, chunk: &str) -> PluginResult<Option<Value>> {
        let _ = chunk;
        Ok(None)
    }
    /// Convert tool definitions (JSON array) into the format's tool schemas.
    fn convert_tools(&self, tools: Value) -> PluginResult<Value> {
        Ok(tools)
    }
    /// Extract tool calls from a parsed result (host `LlmResult` JSON shape).
    fn parse_tool_calls(&self, result: Value) -> PluginResult<Value> {
        let _ = result;
        Ok(Value::Array(Vec::new()))
    }
    /// Describe a token-counting request, or `None` when the format exposes
    /// no counting endpoint (the host falls back to local estimation).
    fn build_count_tokens_request(
        &self,
        request: Value,
        profile: Value,
    ) -> PluginResult<Option<CodecHttpRequest>> {
        let _ = (request, profile);
        Ok(None)
    }
    /// Parse a counting response body into input tokens. No default: a codec
    /// that describes a counting request must implement this explicitly so a
    /// missing implementation fails loudly instead of reporting `0` tokens.
    fn parse_count_tokens_response(&self, body: Value) -> PluginResult<u32> {
        let _ = body;
        Err(crate::error::PluginError::Internal(
            "codec does not implement parse_count_tokens_response".to_string(),
        ))
    }
}

#[async_trait]
pub trait PluginEventHandler: Send + Sync {
    async fn handle(&self, event: PluginEventData) -> PluginResult<()>;
}

// ── provider authentication ───────────────────────────────────────────
// A plugin contributes the login / refresh / resolve contract for one
// provider. The contract is synchronous and JSON-based so Lua tables,
// native dispatch buffers and wasm guest calls share one shape. The host
// owns the login session, the credential store and the request pipeline:
// it calls the plugin on its own schedule and persists the produced
// credential. Plugins stay stateless between calls; all flow state
// travels through the opaque `session` value.
/// Static metadata about a contributed auth implementation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthDescriptor {
    /// Display name, e.g. "Claude (Pro/Max Subscription)".
    pub name: String,
    /// True when access is subscription-backed (non-metered).
    #[serde(default)]
    pub is_subscription: bool,
    /// Selector label for the login option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_label: Option<String>,
}

/// One event rendered to the user during login. Mirrors the host's
/// auth-event vocabulary; the host maps it onto its own interaction type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginAuthEvent {
    /// Informational line.
    Info { message: String },
    /// Authorization URL to open, with optional instructions.
    AuthUrl {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
    },
    /// Device-code challenge (RFC 8628).
    DeviceCode {
        user_code: String,
        verification_uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Progress line.
    Progress { message: String },
}

/// One option of a select prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginAuthOption {
    pub label: String,
    pub value: String,
}

/// One input prompt the host must answer before the flow continues.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginAuthPrompt {
    Text {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Secret {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Select {
        message: String,
        options: Vec<PluginAuthOption>,
    },
    ManualCode {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
}

/// What a login step expects the host to do next.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "next", rename_all = "snake_case")]
pub enum PluginAuthNext {
    /// Waiting on an external event; the host polls.
    Pending,
    /// The host must collect one input, then call submit.
    AwaitingInput { prompt: PluginAuthPrompt },
    /// The flow produced a credential; the host persists it.
    Completed { credential: Value },
    /// The flow failed.
    Failed { error: String },
}

/// The first step of a login flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginLoginChallenge {
    /// Events to render immediately.
    #[serde(default)]
    pub events: Vec<PluginAuthEvent>,
    /// Opaque session state the host passes back on poll / submit.
    pub session: Value,
    /// What the host should do next.
    pub next: PluginAuthNext,
}

/// One transition after a poll or submit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginLoginStep {
    /// Events produced by this step.
    #[serde(default)]
    pub events: Vec<PluginAuthEvent>,
    /// Opaque session state for the following step.
    pub session: Value,
    /// What the host should do next.
    pub next: PluginAuthNext,
}

/// Plugin-contributed provider authentication.
pub trait PluginLlmAuth: Send + Sync {
    /// Static metadata for status UIs and login pickers.
    fn describe(&self) -> PluginResult<AuthDescriptor>;

    /// Begin the login flow: render the challenge and report what the host
    /// should do next. `config` carries the provider's declared auth
    /// parameters (endpoints, client id, scopes, flow) verbatim.
    /// Implementations must not block on the user.
    fn login_start(&self, config: Value) -> PluginResult<PluginLoginChallenge>;

    /// Advance a polling step; the host calls this on an interval while
    /// the flow is pending.
    fn login_poll(&self, session: Value) -> PluginResult<PluginLoginStep>;

    /// Accept user input for a step awaiting it.
    fn login_submit(&self, session: Value, input: String) -> PluginResult<PluginLoginStep>;

    /// Exchange a refresh token; returns the new credential JSON
    /// (`{access, refresh, expires, is_subscription, extra?}`).
    fn refresh(&self, credential: Value) -> PluginResult<Value>;

    /// Derive request auth from a valid credential JSON. The result
    /// carries `api_key` / `headers` / `base_url` / `is_subscription` /
    /// `source`; a null result means not configured.
    fn resolve(&self, credential: Value) -> PluginResult<Value>;
}

// ── image generation ────────────────────────────────────────────────
// A plugin contributes an image generation implementation for one provider.
// The contract is synchronous and JSON-based like the codec/auth
// contracts: `context` carries the resolved endpoint plus credential
// (`{base_url, api_key, model, headers?}`), `prompt` is the text prompt
// and `options` carries `{size?, quality?, count?, response_format?}`.
// The result follows the host `ImagesResult` JSON shape
// (`{images: [{b64_json?, url?, revised_prompt?}], created?}`). Network
// implementations must run on the native backend (same constraint as
// `llm-auth`); Lua/Wasm sandboxes have no HTTP.
pub trait PluginLlmImages: Send + Sync {
    /// Display name, e.g. "Acme Images".
    fn name(&self) -> String;
    /// Generate images for a prompt under the given context and options.
    fn generate_images(
        &self,
        context: Value,
        prompt: String,
        options: Value,
    ) -> PluginResult<Value>;
}

#[async_trait]
pub trait PluginMiddlewareHandler: Send + Sync {
    /// Handle one middleware phase. Implementations may rewrite `context`
    /// before passing it to `next`; the returned value is the final context
    /// after the rest of the chain. Returning without calling `next`
    /// short-circuits the chain with the returned context.
    async fn handle(&self, context: Value, next: NextFn) -> PluginResult<Value>;
}

/// Plugin-contributed compaction planning advice.
///
/// Pure shaping hook for context compression: the engine owns thresholds,
/// version anchors, write-back and audit, the plugin only suggests which
/// ranges to summarize and which small blocks to drop. Input carries a
/// read-only snapshot of the decision (hot token estimate, resolved
/// thresholds, active blocks with sizes, per-message costs, aggressiveness
/// flag and item cap) as JSON; output carries suggested items and drops
/// as JSON. Returning an empty suggestion defers to the built-in planner.
/// Every suggestion is validated and clamped by the engine, so unknown
/// block ids, out-of-range indices and over-cap plans never reach
/// execution. Implementations must stay synchronous and side-effect free.
///
/// Only the native backend dispatches this hook; Lua and Wasm sandboxes
/// expose no compaction channel, mirroring the retrieval extensions below.
pub trait PluginCompactionAdvisor: Send + Sync {
    /// Suggest a compaction plan for one settlement point.
    fn advise(&self, input: Value) -> PluginResult<Value>;
}

// ── retrieval extensions (advisory, native-dispatched) ──
// Advisory plugin shapes for text analysis, retrieval shaping, vector
// encodings and the ingest, routing, postprocess and judge hooks.
// Snapshot-style inputs carry a `protocol_version` (currently one);
// typed text-plus-options hooks evolve through `options` instead.
// The host owns translation and validation; implementations stay
// synchronous and side-effect free so the default chain never changes.
// Only the native backend dispatches these; Lua/Wasm hosts expose no
// retrieval channel.
/// Plugin-contributed text analyzer for full-text filters.
///
/// Pure tokenize and normalize hook: the host applies one analyzer
/// definition to memory scans, embedded scans and server text index configuration so
/// the three backends stay consistent. Network access is forbidden.
pub trait PluginTextAnalyzer: Send + Sync {
    /// Analyzer name referenced by a filter policy.
    fn name(&self) -> String;
    /// Split `text` into search tokens; empty output matches nothing.
    fn analyze(&self, text: String) -> PluginResult<Vec<String>>;
}

/// Plugin-contributed retrieval shaping advice for grouping,
/// recommend, prefetch fusion and sorting inputs.
///
/// Pure shaping hook following the compaction advisor pattern: input
/// carries a read-only snapshot of candidate lists as JSON, output
/// carries the fused, grouped or reordered suggestion as JSON. Returning
/// an empty suggestion defers to the built-in path. Every suggestion is
/// validated by the engine, so unknown ids and out-of-range ranks never
/// reach execution. Implementations must stay synchronous.
pub trait PluginRetrievalAdvisor: Send + Sync {
    /// Suggest fused, grouped or reordered hits for one retrieval.
    fn advise(&self, input: Value) -> PluginResult<Value>;
}

/// Plugin-contributed vector encoding for sparse and
/// multi-vector shapes.
///
/// Pure encoding hook: the host owns the storage contract and dimension
/// checks, the plugin only maps text plus options to a sparse or
/// multi-vector JSON shape. Absent encodings keep the dense path
/// unchanged. Implementations must stay synchronous and side-effect free.
pub trait PluginVectorEncoding: Send + Sync {
    /// Encoding name referenced by a collection policy.
    fn name(&self) -> String;
    /// Encode `text` with `options` into a sparse or multi-vector JSON shape.
    fn encode(&self, text: String, options: Value) -> PluginResult<Value>;
}

/// Plugin-contributed metadata extraction for ingest enrichment.
///
/// Pure mapping hook: the host owns chunking and writes, the plugin only
/// suggests string metadata merged onto the document. Output must be a JSON
/// object with scalar values; absent extractors keep the input metadata
/// unchanged. Implementations must stay synchronous and side-effect free.
pub trait PluginMetadataExtractor: Send + Sync {
    /// Extractor name referenced by an ingest policy.
    fn name(&self) -> String;
    /// Suggest metadata for `text` with `options` as a JSON object.
    fn extract(&self, text: String, options: Value) -> PluginResult<Value>;
}

/// Plugin-contributed chunk splitting for semantic strategies.
///
/// Pure splitting hook: the host owns embedding and writes, the plugin only
/// suggests content pieces. Output must be a JSON array of objects carrying
/// a non-empty `content` string; absent splitters keep the built-in splitter.
/// Implementations must stay synchronous and side-effect free.
pub trait PluginChunkSplitter: Send + Sync {
    /// Splitter name referenced by an ingest policy.
    fn name(&self) -> String;
    /// Split `text` with `options` into a JSON array of content pieces.
    fn split(&self, text: String, options: Value) -> PluginResult<Value>;
}

/// Plugin-contributed route selection for semantic routing.
///
/// Pure selection hook following the retrieval advisor pattern: input carries
/// a read-only snapshot of the query plus candidate target names, output
/// carries the selected subset. Unknown names are skipped, an empty
/// suggestion defers to the full candidate list. Implementations must stay
/// synchronous and side-effect free.
pub trait PluginRouteSelector: Send + Sync {
    /// Suggest the target subset for one retrieval input.
    fn select(&self, input: Value) -> PluginResult<Value>;
}

/// Plugin-contributed postprocessing for diversity and filtering.
///
/// Pure shaping hook following the retrieval advisor pattern: input carries
/// a read-only snapshot of the query plus ranked candidates, output carries
/// the reordered or filtered suggestion. Every suggestion is validated so
/// unknown ids never reach execution. Implementations must stay synchronous
/// and side-effect free.
pub trait PluginPostprocessor: Send + Sync {
    /// Suggest postprocessed order for one retrieval input.
    fn process(&self, input: Value) -> PluginResult<Value>;
}

/// Plugin-contributed evaluation judge for answer scoring.
///
/// Pure scoring hook: input carries the query, the answer and the contexts,
/// output carries a zero-to-one score plus an optional reason. Out-of-range
/// scores are rejected and the caller keeps its heuristic score.
/// Implementations must stay synchronous and side-effect free.
pub trait PluginEvalJudge: Send + Sync {
    /// Score one answer for an evaluation input.
    fn judge(&self, input: Value) -> PluginResult<Value>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outcome_bool_is_legacy_proceed_flag() {
        let incoming = json!({"a": 1});
        let out = parse_middleware_outcome(&json!(true), &incoming);
        assert_eq!(
            out,
            MiddlewareOutcome {
                proceed: true,
                context: incoming.clone(),
            }
        );
        let out = parse_middleware_outcome(&json!(false), &incoming);
        assert!(!out.proceed);
        assert_eq!(out.context, incoming);
    }

    #[test]
    fn outcome_envelope_rewrites_context() {
        let incoming = json!({"a": 1});
        let response = json!({"proceed": true, "context": {"a": 2}});
        let out = parse_middleware_outcome(&response, &incoming);
        assert!(out.proceed);
        assert_eq!(out.context, json!({"a": 2}));

        let response = json!({"proceed": false, "context": {"stop": true}});
        let out = parse_middleware_outcome(&response, &incoming);
        assert!(!out.proceed);
        assert_eq!(out.context, json!({"stop": true}));
    }

    #[test]
    fn outcome_envelope_fields_default() {
        let incoming = json!({"a": 1});
        let out = parse_middleware_outcome(&json!({"context": {"b": 2}}), &incoming);
        assert!(out.proceed);
        assert_eq!(out.context, json!({"b": 2}));

        let out = parse_middleware_outcome(&json!({"proceed": false}), &incoming);
        assert!(!out.proceed);
        assert_eq!(out.context, incoming);
    }

    #[test]
    fn cli_input_outcome_passes_rewrites_and_takes_over_in_order() {
        assert_eq!(
            parse_cli_input_outcome(&json!(true), "hi"),
            CliInputOutcome::Pass
        );
        assert_eq!(
            parse_cli_input_outcome(&json!(false), "hi"),
            CliInputOutcome::Takeover(String::new())
        );
        assert_eq!(
            parse_cli_input_outcome(&json!({"action": "rewrite", "text": "/help"}), "hi"),
            CliInputOutcome::Rewrite("/help".to_string())
        );
        assert_eq!(
            parse_cli_input_outcome(&json!({"action": "takeover", "echo": "done"}), "hi"),
            CliInputOutcome::Takeover("done".to_string())
        );
        assert_eq!(
            parse_cli_input_outcome(&json!(null), "hi"),
            CliInputOutcome::Pass
        );
        let (text, echo) = apply_cli_input_outcome("hi", CliInputOutcome::Rewrite("/help".into()));
        assert_eq!(text, "/help");
        assert_eq!(echo, None);
        let (_, echo) = apply_cli_input_outcome("hi", CliInputOutcome::Takeover("done".into()));
        assert_eq!(echo.as_deref(), Some("done"));
    }

    #[test]
    fn outcome_unknown_shapes_fall_back_to_legacy() {
        let incoming = json!({"a": 1});
        for response in [
            json!(null),
            json!(42),
            json!("x"),
            json!([1]),
            json!({"other": 1}),
        ] {
            let out = parse_middleware_outcome(&response, &incoming);
            assert!(out.proceed, "response: {response}");
            assert_eq!(out.context, incoming, "response: {response}");
        }
    }

    fn cli_command() -> PluginCliCommand {
        PluginCliCommand {
            id: "review.run".to_string(),
            title: "Run review".to_string(),
            group: "Review".to_string(),
            description: "Review the current diff".to_string(),
            slash_name: Some("review".to_string()),
            slash_aliases: vec!["rev".to_string()],
            slash_argument: "[path]".to_string(),
            palette: true,
            key_hint: None,
            when: PluginCliCondition::default(),
        }
    }

    #[test]
    fn cli_command_accepts_a_routable_definition() {
        assert!(cli_command().validate().is_ok());
    }

    #[test]
    fn cli_command_rejects_unroutable_definitions() {
        let mut bad = cli_command();
        bad.id = "has space".to_string();
        assert!(bad.validate().is_err());
        let mut bad = cli_command();
        bad.slash_name = None;
        assert!(bad.validate().is_err());
        let mut bad = cli_command();
        bad.slash_aliases = vec!["/lead".to_string()];
        assert!(bad.validate().is_err());
        let mut ok = cli_command();
        ok.palette = false;
        ok.slash_name = None;
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn completion_source_rejects_missing_triggers() {
        let source = PluginCompletionSource {
            id: "issues".to_string(),
            trigger_chars: vec!["#".to_string()],
            items: vec![PluginCompletionItem {
                value: "#12".to_string(),
                label: None,
                description: None,
            }],
        };
        assert!(source.validate().is_ok());
        let empty = PluginCompletionSource {
            id: "issues".to_string(),
            trigger_chars: vec![],
            items: vec![],
        };
        assert!(empty.validate().is_err());
    }

    #[test]
    fn setting_field_requires_dotted_key_and_known_fallback() {
        let field = PluginSettingField {
            key: "review.style".to_string(),
            label: "Review style".to_string(),
            values: vec!["short".to_string(), "full".to_string()],
            fallback: "short".to_string(),
        };
        assert!(field.validate().is_ok());
        let bad = PluginSettingField {
            key: "nodot".to_string(),
            ..field.clone()
        };
        assert!(bad.validate().is_err());
        let bad = PluginSettingField {
            fallback: "other".to_string(),
            ..field.clone()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn tool_summary_renders_placeholders_literally() {
        let summary = PluginToolSummary {
            tool_name: "grep".to_string(),
            call_template: "search {args}".to_string(),
            result_template: "{result} hits".to_string(),
        };
        assert!(summary.validate().is_ok());
        assert_eq!(
            render_tool_summary(&summary.call_template, "-r foo", ""),
            "search -r foo"
        );
        assert_eq!(
            render_tool_summary(&summary.result_template, "", "12"),
            "12 hits"
        );
        assert_eq!(render_tool_summary("{oops}", "a", "b"), "{oops}");
    }

    #[test]
    fn markdown_rules_chain_in_order_and_skip_empty() {
        let rules = vec![
            PluginMarkdownRule {
                id: "arrows".to_string(),
                pattern: "-->".to_string(),
                replacement: "→".to_string(),
                surfaces: vec![],
            },
            PluginMarkdownRule {
                id: "empty".to_string(),
                pattern: String::new(),
                replacement: "gone".to_string(),
                surfaces: vec![],
            },
        ];
        assert!(rules[0].validate().is_ok());
        assert!(rules[1].validate().is_err());
        assert_eq!(apply_markdown_rules("a --> b", &rules), "a → b");
    }
}
