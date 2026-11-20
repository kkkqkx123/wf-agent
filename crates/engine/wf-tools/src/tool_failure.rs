//! The single model-facing failure envelope.
//!
//! Every tool failure that reaches the model is serialized here: one JSON
//! object under an `error` key carrying a stable `code`, a one-line
//! `message`, an optional `hint`, the retryability bit and an optional
//! `detail` blob. Producers never concatenate their own prefixes; the
//! execution pipeline keeps typed information instead of flattening it
//! into prose, and this module is the only place that turns it into text.
//!
//! `detail` holds the raw producer text (io/serde errors, upstream server
//! output, per-item failure lists) and goes through the same overflow
//! policy as ordinary tool output, so a failure can never blow up the
//! context window while the full text stays readable on disk.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::error::ToolError;
use crate::tool_output::{resolve_spill_dir, truncate_with_spill};

/// Metadata key marking a tool message as failed.
pub const ERROR_METADATA_KEY: &str = "is_error";
/// Metadata key carrying the failure code for statistics and frontends.
pub const ERROR_CODE_METADATA_KEY: &str = "error_code";

/// Upper bound of the inline one-line message.
pub const MAX_MESSAGE_CHARS: usize = 200;
/// Upper bound of the inline actionable hint.
pub const MAX_HINT_CHARS: usize = 300;

/// Stable failure classes exposed to the model and to frontends.
///
/// The set is a protocol contract: adding a variant changes the wire
/// vocabulary, so frontends can map codes to localized wording without
/// parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFailureCode {
    /// The referenced file, directory or tool does not exist.
    NotFound,
    /// The path is outside the allowed execution scope.
    ScopeDenied,
    /// Arguments are missing, malformed or violate the tool contract.
    ParamInvalid,
    /// The target is protected and needs explicit approval.
    WriteProtected,
    /// A patch or diff could not be applied.
    PatchFailed,
    /// An approval gate or hook denied the call.
    ApprovalDenied,
    /// The tool is blocked for this execution and must not be retried.
    ToolBlocked,
    /// The call exceeded its time budget.
    Timeout,
    /// The call was cancelled by the execution.
    Cancelled,
    /// An upstream service (MCP server, HTTP endpoint) failed.
    UpstreamError,
    /// Some operations succeeded and some failed.
    PartialFailure,
    /// The tool reported a domain failure through its payload.
    ToolReported,
    /// Engine-side fault: serialization, io, wiring.
    Internal,
}

impl ToolFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolFailureCode::NotFound => "not_found",
            ToolFailureCode::ScopeDenied => "scope_denied",
            ToolFailureCode::ParamInvalid => "param_invalid",
            ToolFailureCode::WriteProtected => "write_protected",
            ToolFailureCode::PatchFailed => "patch_failed",
            ToolFailureCode::ApprovalDenied => "approval_denied",
            ToolFailureCode::ToolBlocked => "tool_blocked",
            ToolFailureCode::Timeout => "timeout",
            ToolFailureCode::Cancelled => "cancelled",
            ToolFailureCode::UpstreamError => "upstream_error",
            ToolFailureCode::PartialFailure => "partial_failure",
            ToolFailureCode::ToolReported => "tool_reported_failure",
            ToolFailureCode::Internal => "internal",
        }
    }

    /// Parse a producer-side tag into a code. Producers encode the class as
    /// a leading tag so the classification survives the trip through a
    /// string-only error type; the tag never reaches the model.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let code = match tag {
            "not_found" => ToolFailureCode::NotFound,
            "scope_violation" => ToolFailureCode::ScopeDenied,
            "param_invalid" => ToolFailureCode::ParamInvalid,
            "write_protected" => ToolFailureCode::WriteProtected,
            "patch_failed" => ToolFailureCode::PatchFailed,
            "approval_denied" => ToolFailureCode::ApprovalDenied,
            "tool_blocked" => ToolFailureCode::ToolBlocked,
            "timeout" => ToolFailureCode::Timeout,
            "cancelled" => ToolFailureCode::Cancelled,
            "upstream_error" => ToolFailureCode::UpstreamError,
            "partial_failure" => ToolFailureCode::PartialFailure,
            "tool_reported_failure" => ToolFailureCode::ToolReported,
            "internal" => ToolFailureCode::Internal,
            _ => return None,
        };
        Some(code)
    }

    /// Partial failures are repaired by the model, not retried blindly, and
    /// they must not trip the per-tool failure breaker: a tool that keeps
    /// succeeding partially is still usable.
    pub fn is_partial(self) -> bool {
        self == ToolFailureCode::PartialFailure
    }

    /// Retryability implied by the class alone, used when no typed error is
    /// available (payload failures, producer-tagged reasons).
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            ToolFailureCode::Timeout | ToolFailureCode::UpstreamError | ToolFailureCode::Internal
        )
    }
}

/// One failure as it is shown to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolFailure {
    pub code: ToolFailureCode,
    pub message: String,
    pub hint: Option<String>,
    pub retryable: bool,
    pub detail: Option<String>,
    pub context: Option<Value>,
    /// Tool name for carriers that do not put it on the wire (the `general`
    /// proxy returns several calls in one payload).
    pub tool: Option<String>,
}

impl ToolFailure {
    pub fn new(code: ToolFailureCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            retryable: default_retryable_for(code),
            detail: None,
            context: None,
            tool: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_context(mut self, context: Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Build a failure from a typed execution error. The raw error text is
    /// preserved as `detail`; the model-facing message is the same text with
    /// producer tags and engine prefixes stripped.
    pub fn from_tool_error(tool: Option<&str>, err: &ToolError) -> Self {
        let (mut code, text) = classify(err);
        let (tagged, stripped) = strip_tags(&text);
        if let Some(tagged) = tagged {
            code = tagged;
        }
        // Multi-line reasons keep their first line as the headline and move
        // the body to `detail`, so an aggregated failure list stays legible
        // instead of being flattened into one clipped line.
        let (headline, tail) = match stripped.split_once('\n') {
            Some((head, rest)) => (head.trim().to_string(), Some(rest.trim().to_string())),
            None => (stripped.clone(), None),
        };
        let (message, hint) = split_reason(&headline);
        let mut failure = ToolFailure::new(code, message.clone());
        if let Some(hint) = hint.as_deref() {
            failure.hint = Some(hint.to_string());
        }
        failure.detail = match tail {
            Some(tail) if !tail.is_empty() => Some(tail),
            _ => {
                // Keep the raw text only when it carries information the
                // rendered message and hint cannot: never re-introduce the
                // engine's own prefixes as duplicate prose.
                let rendered = format!("{} {}", message, hint.unwrap_or_default());
                if collapse_whitespace(&stripped) != collapse_whitespace(&rendered) {
                    Some(stripped)
                } else {
                    None
                }
            }
        };
        failure.retryable = default_retryable(err);
        if let Some(tool) = tool {
            failure.tool = Some(tool.to_string());
        }
        failure
    }

    /// Build a failure from a failed execution result.
    ///
    /// A payload that reported `success: false` keeps its per-item detail
    /// and failed indexes; a bare error string falls back to the result's
    /// stable code. Retryability follows the result when the executor set
    /// it and the class otherwise.
    pub fn from_result(tool: Option<&str>, result: &wf_types::tool::ToolExecutionResult) -> Self {
        if let Some(failure) = result
            .result
            .as_ref()
            .and_then(|payload| ToolFailure::from_payload(tool, payload))
        {
            return failure;
        }
        let code = result
            .error_code
            .as_deref()
            .and_then(ToolFailureCode::from_tag);
        let reason = result.error.as_deref().unwrap_or("tool reported failure");
        ToolFailure::from_reason(tool, code, reason, result.retryable)
    }

    /// Build a failure from a bare reason string plus its class, for
    /// carriers that only keep text (result payloads, proxy items).
    pub fn from_reason(
        tool: Option<&str>,
        code: Option<ToolFailureCode>,
        reason: &str,
        retryable: Option<bool>,
    ) -> Self {
        let (tagged, stripped) = strip_tags(reason);
        let code = tagged.or(code).unwrap_or(ToolFailureCode::ToolReported);
        let (headline, tail) = match stripped.split_once('\n') {
            Some((head, rest)) => (head.trim().to_string(), Some(rest.trim().to_string())),
            None => (stripped.clone(), None),
        };
        let (message, hint) = split_reason(&headline);
        let mut failure = ToolFailure::new(code, message.clone());
        if let Some(hint) = hint.as_deref() {
            failure.hint = Some(hint.to_string());
        }
        failure.detail = match tail {
            Some(tail) if !tail.is_empty() => Some(tail),
            _ => None,
        };
        failure.retryable = retryable.unwrap_or_else(|| code.default_retryable());
        if let Some(tool) = tool {
            failure.tool = Some(tool.to_string());
        }
        failure
    }

    /// The `{"error": {...}}` object as a JSON value, for carriers that
    /// nest the failure inside their own payload (`general` batch items).
    pub fn to_value(&self, workdir: Option<&str>) -> Value {
        let text = self.render(workdir).text;
        serde_json::from_str(&text)
            .unwrap_or_else(|_| serde_json::json!({ "error": { "message": self.message } }))
    }

    /// Build a failure from a payload that reported `success: false`.
    ///
    /// Handlers keep returning their own JSON on partial success so the
    /// model still sees per-item outcomes; the pipeline only needs the
    /// class, the headline and the failed item indexes.
    pub fn from_payload(tool: Option<&str>, payload: &Value) -> Option<Self> {
        let success = payload.get("success").and_then(Value::as_bool)?;
        if success {
            return None;
        }
        let code = payload
            .get("error_code")
            .and_then(Value::as_str)
            .and_then(ToolFailureCode::from_tag)
            .unwrap_or(ToolFailureCode::ToolReported);
        let reason = payload
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_failure_message(code));
        let (tagged, stripped) = strip_tags(&reason);
        let code = tagged.unwrap_or(code);
        let (message, hint) = split_reason(&stripped);
        let mut failure = ToolFailure::new(code, message);
        if let Some(hint) = hint {
            failure.hint = Some(hint);
        }
        let detail = payload
            .get("detail")
            .and_then(Value::as_str)
            .or_else(|| payload.get("display").and_then(Value::as_str))
            .map(str::to_string)
            .filter(|detail| collapse_whitespace(detail) != collapse_whitespace(&failure.message));
        failure.detail = detail;
        if let Some(indices) = payload.get("failed_indices").and_then(Value::as_array) {
            failure.context = Some(serde_json::json!({ "failed_indices": indices }));
        }
        if let Some(tool) = tool {
            failure.tool = Some(tool.to_string());
        }
        failure.retryable = false;
        Some(failure)
    }

    /// Render the model-visible text: a single JSON object under `error`,
    /// followed by nothing else.
    pub fn render(&self, workdir: Option<&str>) -> RenderedFailure {
        let (message, clipped) = clip(&self.message, MAX_MESSAGE_CHARS);
        let mut detail = self.detail.clone();
        if clipped {
            // A clipped headline would drop information; the full text stays
            // reachable through the detail channel instead of being lost.
            detail = Some(match detail {
                Some(existing) => format!("{}\n{}", self.message, existing),
                None => self.message.clone(),
            });
        }

        let mut overflow = None;
        let rendered_detail = detail.map(|text| {
            let oversize = text.len() > crate::tool_output::MAX_OUTPUT_BYTES;
            let outcome = truncate_with_spill(&text, &resolve_spill_dir(workdir));
            let kept = outcome.kept_text.clone();
            if oversize || outcome.spilled_path.is_some() {
                overflow = Some(outcome);
            }
            kept
        });

        let mut object = Map::new();
        object.insert(
            "code".to_string(),
            Value::String(self.code.as_str().to_string()),
        );
        object.insert("message".to_string(), Value::String(message));
        if let Some(hint) = self.hint.as_deref() {
            object.insert(
                "hint".to_string(),
                Value::String(clip(hint, MAX_HINT_CHARS).0),
            );
        }
        object.insert("retryable".to_string(), Value::Bool(self.retryable));
        if let Some(detail) = rendered_detail {
            object.insert("detail".to_string(), Value::String(detail));
        }
        if let Some(context) = self.context.clone() {
            object.insert("context".to_string(), context);
        }
        if let Some(tool) = self.tool.as_deref() {
            object.insert("tool".to_string(), Value::String(tool.to_string()));
        }

        let text = serde_json::json!({ "error": Value::Object(object) }).to_string();
        RenderedFailure {
            text,
            truncated: overflow.as_ref().map(|_| true),
            output_path: overflow
                .and_then(|outcome| outcome.spilled_path)
                .map(|path| path.to_string_lossy().to_string()),
        }
    }

    /// Write the machine-readable bits the model does not need to parse.
    pub fn apply_metadata(&self, metadata: &mut Option<HashMap<String, Value>>) {
        let map = metadata.get_or_insert_with(HashMap::new);
        map.insert(ERROR_METADATA_KEY.to_string(), Value::Bool(true));
        map.insert(
            ERROR_CODE_METADATA_KEY.to_string(),
            Value::String(self.code.as_str().to_string()),
        );
        if self.code == ToolFailureCode::ScopeDenied {
            map.insert(
                crate::error::SCOPE_VIOLATION_METADATA_KEY.to_string(),
                Value::Bool(true),
            );
        }
    }
}

/// Rendered failure plus the overflow bookkeeping the message assembler
/// copies onto the result.
pub struct RenderedFailure {
    pub text: String,
    pub truncated: Option<bool>,
    pub output_path: Option<String>,
}

impl RenderedFailure {
    /// Stamp the machine-readable bits of one failure onto a tool message.
    pub fn apply_message_metadata(
        &self,
        metadata: &mut Option<wf_types::Metadata>,
        failure: &ToolFailure,
    ) {
        failure.apply_metadata(metadata);
        if self.truncated != Some(true) {
            return;
        }
        let map = metadata.get_or_insert_with(Default::default);
        map.insert(
            crate::tool_output::TRUNCATED_METADATA_KEY.to_string(),
            Value::Bool(true),
        );
        if let Some(path) = self.output_path.as_deref() {
            map.insert(
                crate::tool_output::OUTPUT_PATH_METADATA_KEY.to_string(),
                Value::String(path.to_string()),
            );
        }
    }

    pub fn apply_overflow_metadata(&self, result: &mut wf_types::tool::ToolExecutionResult) {
        if let Some(truncated) = self.truncated {
            result.truncated = Some(truncated);
        }
        if let Some(path) = self.output_path.as_deref() {
            result.output_path = Some(path.to_string());
        }
    }
}

/// Type-directed classification, used by executors to keep the class on the
/// result and by the envelope to pick its `code`. Producer tags win over
/// this table when present.
pub fn classify(err: &ToolError) -> (ToolFailureCode, String) {
    let (code, text) = match err {
        ToolError::NotFound(text) => (ToolFailureCode::NotFound, text.clone()),
        ToolError::ValidationFailed(text) => (ToolFailureCode::ParamInvalid, text.clone()),
        ToolError::ScopeViolation { detail, .. } => (ToolFailureCode::ScopeDenied, detail.clone()),
        ToolError::ExecutionFailed { reason, .. } => {
            // Rejections produced by the pipeline (visibility gate, failure
            // protection) carry a fixed marker phrase; everything else is a
            // handler-level failure.
            let code = if reason.contains("is blocked in this execution") {
                ToolFailureCode::ToolBlocked
            } else {
                ToolFailureCode::ToolReported
            };
            (code, reason.clone())
        }
        ToolError::Timeout {
            tool_id,
            timeout_ms,
        } => (
            ToolFailureCode::Timeout,
            format!("tool '{tool_id}' timed out after {timeout_ms}ms"),
        ),
        ToolError::Cancelled { tool_id } => (
            ToolFailureCode::Cancelled,
            format!("tool '{tool_id}' was cancelled"),
        ),
        ToolError::RetryExhausted { tool_id, retries } => (
            ToolFailureCode::ToolReported,
            format!("tool '{tool_id}' failed after {retries} retries"),
        ),
        ToolError::McpError(text) | ToolError::TransportError(text) => {
            (ToolFailureCode::UpstreamError, text.clone())
        }
        ToolError::ConnectionFailed { server, reason } => (
            ToolFailureCode::UpstreamError,
            format!("MCP server '{server}' is unavailable: {reason}"),
        ),
        ToolError::RestError { url, status } => (
            ToolFailureCode::UpstreamError,
            format!("HTTP {status} from {url}"),
        ),
        ToolError::HttpError(err) => (ToolFailureCode::UpstreamError, err.to_string()),
        ToolError::Io(err) => (ToolFailureCode::Internal, err.to_string()),
        ToolError::Serialization(err) => (ToolFailureCode::Internal, err.to_string()),
        ToolError::CallbackNotRegistered(text) => (ToolFailureCode::Internal, text.clone()),
        ToolError::Internal(text) => (ToolFailureCode::Internal, text.clone()),
        ToolError::ExecutionError(text) => (ToolFailureCode::Internal, text.clone()),
    };
    (code, text)
}

/// Default retryability per execution error, mirroring the engine
/// classification. The agent engine overrides this with its own analysis;
/// this copy exists so carriers without an engine (workflow nodes, MCP
/// bridging) still render an honest bit, and a cross-crate test pins the
/// two to the same answers.
pub fn default_retryable(err: &ToolError) -> bool {
    match err {
        ToolError::Timeout { .. } => true,
        ToolError::Cancelled { .. } => false,
        ToolError::NotFound(_) | ToolError::ValidationFailed(_) => false,
        ToolError::RestError { status, .. } => matches!(status, 429 | 500..=599),
        ToolError::HttpError(err) => match err.status() {
            Some(status) => matches!(status.as_u16(), 429 | 500..=599),
            None => true,
        },
        ToolError::ConnectionFailed { .. } | ToolError::TransportError(_) => true,
        ToolError::McpError(_) | ToolError::Io(_) => true,
        _ => false,
    }
}

fn default_retryable_for(code: ToolFailureCode) -> bool {
    code.default_retryable()
}

fn default_failure_message(code: ToolFailureCode) -> String {
    match code {
        ToolFailureCode::PartialFailure => "some operations failed".to_string(),
        _ => "tool reported failure".to_string(),
    }
}

/// Strip producer tags from a reason, returning the first recognized tag.
fn strip_tags(reason: &str) -> (Option<ToolFailureCode>, String) {
    let mut rest = reason.trim();
    let mut found = None;
    while let Some((head, tail)) = rest.split_once(": ") {
        let Some(code) = ToolFailureCode::from_tag(head.trim()) else {
            break;
        };
        if found.is_none() {
            found = Some(code);
        }
        rest = tail.trim();
    }
    (found, rest.to_string())
}

/// Sentence-level lead words that start an actionable hint.
const HINT_LEAD_WORDS: &[&str] = &[
    "Add", "Call", "Check", "Choose", "Create", "Include", "Increase", "Keep", "Merge", "Narrow",
    "Pass", "Prefer", "Provide", "Re-read", "Reduce", "Retry", "Run", "Split", "Target", "Try",
    "Use", "Verify",
];

/// Split a reason into a one-line message and an actionable hint.
///
/// Producers write `<fact>. <Imperative ...>`; the split is deterministic so
/// the same reason always renders the same way, and a reason without such a
/// boundary keeps its guidance inside the message.
fn split_reason(reason: &str) -> (String, Option<String>) {
    let collapsed = collapse_whitespace(reason);
    if collapsed.is_empty() {
        return ("tool reported failure".to_string(), None);
    }
    let bytes = collapsed.as_bytes();
    let mut search = 0usize;
    while let Some(offset) = collapsed[search..].find(". ") {
        let boundary = search + offset + 2;
        let tail = &collapsed[boundary..];
        let first_word = tail.split([' ', ',', ':']).next().unwrap_or_default();
        if HINT_LEAD_WORDS.contains(&first_word) {
            let message = collapsed[..boundary - 1].trim_end().to_string();
            let hint = tail.trim().to_string();
            if !message.is_empty() && !hint.is_empty() {
                return (message, Some(hint));
            }
        }
        search = boundary;
        if search >= bytes.len() {
            break;
        }
    }
    (collapsed, None)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Clip to a character budget, reporting whether anything was dropped.
fn clip(text: &str, limit: usize) -> (String, bool) {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= limit {
            out.push('…');
            return (out, true);
        }
        out.push(ch);
    }
    (out, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NOT_FOUND_TAG;

    #[test]
    fn validation_failure_carries_no_internal_prefix() {
        let err = ToolError::ValidationFailed("Missing required parameter: path".to_string());
        let failure = ToolFailure::from_tool_error(Some("edit_file"), &err);
        assert_eq!(failure.code, ToolFailureCode::ParamInvalid);
        assert_eq!(failure.message, "Missing required parameter: path");
        assert!(failure.hint.is_none());
        assert!(!failure.retryable);
        let rendered = failure.render(None);
        assert!(rendered
            .text
            .starts_with(r#"{"error":{"code":"param_invalid""#));
        assert!(!rendered.text.contains("Tool validation failed"));
    }

    #[test]
    fn producer_tag_becomes_code_and_never_reaches_text() {
        let err = ToolError::ExecutionError(crate::error::tag_detail(
            NOT_FOUND_TAG,
            "file '/w/a.rs' does not exist. Use read_file to confirm the path.".to_string(),
        ));
        let failure = ToolFailure::from_tool_error(Some("read_file"), &err);
        assert_eq!(failure.code, ToolFailureCode::NotFound);
        assert_eq!(failure.message, "file '/w/a.rs' does not exist.");
        assert_eq!(
            failure.hint.as_deref(),
            Some("Use read_file to confirm the path.")
        );
        let rendered = failure.render(None);
        assert!(!rendered.text.contains("not_found:"));
        assert!(rendered.text.contains("\"code\":\"not_found\""));
        assert!(rendered.text.contains("\"hint\":\"Use read_file"));
    }

    #[test]
    fn scope_violation_keeps_its_code_and_metadata() {
        let err = ToolError::scope_violation(
            "write_file",
            "path '/etc/passwd' is outside write prefixes.".to_string(),
        );
        let failure = ToolFailure::from_tool_error(Some("write_file"), &err);
        assert_eq!(failure.code, ToolFailureCode::ScopeDenied);
        assert!(!rendered_detail(&failure).contains("scope_violation:"));
        let mut metadata = None;
        failure.apply_metadata(&mut metadata);
        let metadata = metadata.expect("metadata");
        assert_eq!(metadata[ERROR_METADATA_KEY], Value::Bool(true));
        assert_eq!(
            metadata[ERROR_CODE_METADATA_KEY],
            Value::String("scope_denied".to_string())
        );
        assert_eq!(
            metadata[crate::error::SCOPE_VIOLATION_METADATA_KEY],
            Value::Bool(true)
        );
    }

    fn rendered_detail(failure: &ToolFailure) -> String {
        failure.render(None).text
    }

    #[test]
    fn payload_failure_maps_to_partial_with_indexes() {
        let payload = serde_json::json!({
            "success": false,
            "error": "1 of 3 patch operations failed",
            "display": "Failed to update 'a.rs': could not find chunk",
            "failed_indices": [2],
        });
        let failure = ToolFailure::from_payload(Some("apply_patch"), &payload)
            .expect("payload reports failure");
        assert_eq!(failure.code, ToolFailureCode::ToolReported);
        assert_eq!(failure.message, "1 of 3 patch operations failed");
        assert!(failure.detail.unwrap().contains("could not find chunk"));
        assert_eq!(
            failure.context,
            Some(serde_json::json!({"failed_indices": [2]}))
        );
        assert!(!failure.retryable);
    }

    #[test]
    fn partial_code_rides_the_payload() {
        let payload = serde_json::json!({
            "success": false,
            "error_code": "partial_failure",
            "error": "2 of 5 diff blocks failed",
        });
        let failure = ToolFailure::from_payload(Some("apply_diff"), &payload).expect("failure");
        assert_eq!(failure.code, ToolFailureCode::PartialFailure);
        assert!(failure.code.is_partial());
    }

    #[test]
    fn long_detail_spills_and_keeps_pointer() {
        let dir = std::env::temp_dir().join(format!("wf-failure-spill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let failure = ToolFailure::new(ToolFailureCode::UpstreamError, "server exploded")
            .with_detail("stack line\n".repeat(5000));
        let rendered = failure.render(Some(dir.to_str().expect("utf8")));
        assert_eq!(rendered.truncated, Some(true));
        let path = rendered.output_path.expect("spill path");
        assert!(rendered.text.contains("full output saved to"));
        assert!(std::fs::read_to_string(&path)
            .expect("spill readable")
            .contains("stack line"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retryable_table_matches_error_classes() {
        assert!(default_retryable(&ToolError::Timeout {
            tool_id: "t".to_string(),
            timeout_ms: 1
        }));
        assert!(!default_retryable(&ToolError::ValidationFailed(
            "x".to_string()
        )));
        assert!(default_retryable(&ToolError::McpError("down".to_string())));
        assert!(default_retryable(&ToolError::RestError {
            url: "u".to_string(),
            status: 503
        }));
        assert!(!default_retryable(&ToolError::RestError {
            url: "u".to_string(),
            status: 400
        }));
    }

    #[test]
    fn hint_split_stops_at_the_first_guidance_sentence() {
        let (message, hint) = split_reason(
            "old_string not found in '/w/a.rs'. Provide more context in old_string to make it unique, or use apply_diff for complex edits.",
        );
        assert_eq!(message, "old_string not found in '/w/a.rs'.");
        assert_eq!(
            hint.as_deref(),
            Some("Provide more context in old_string to make it unique, or use apply_diff for complex edits.")
        );
    }

    #[test]
    fn reason_without_guidance_stays_in_the_message() {
        let (message, hint) = split_reason("Command failed with exit code 1");
        assert_eq!(message, "Command failed with exit code 1");
        assert!(hint.is_none());
    }

    #[test]
    fn overlong_message_moves_to_detail() {
        let long = "x".repeat(MAX_MESSAGE_CHARS + 50);
        let failure = ToolFailure::new(ToolFailureCode::Internal, long.clone());
        let rendered = failure.render(None);
        assert!(rendered.text.contains('…'), "headline is clipped");
        assert!(
            rendered.text.contains(&long),
            "clipped remainder stays reachable through detail"
        );
    }
}
