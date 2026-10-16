//! Configurable copy policy and copy-target resolution.
//!
//! [`CopyPolicy`] is the single startup-resolved fact about clipboard
//! writes, built next to [`crate::perf::TuiPerfPolicy`]: the configured
//! default target, whether OSC 52 may emit bytes, and the user-facing
//! reason when it may not. Target resolution ([`CopyTargetSpec`],
//! [`resolve_copy_text`]) is a pure function over source text, so every
//! branch is unit-testable without a terminal.

use tui_terminal::capabilities::{osc52_override, TerminalCapabilities};

/// Default copy target from configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CopyDefaultTarget {
    /// The whole last assistant message.
    #[default]
    Message,
    /// The last fenced code block of the last assistant message.
    Code,
}

/// Why a copy request cannot emit bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDenyReason {
    /// The channel is gated off: no emulator evidence or an explicit
    /// disable override.
    Disabled,
    /// Output is piped or redirected: emitting escape bytes would pollute
    /// the stream.
    NoTerminal,
}

impl CopyDenyReason {
    /// User-facing notice for the denial.
    pub fn notice(self) -> &'static str {
        match self {
            CopyDenyReason::Disabled => {
                "copy unavailable: terminal clipboard not supported (WF_TUI_OSC52_CLIPBOARD=1 forces it)"
            }
            CopyDenyReason::NoTerminal => "copy unavailable: not a terminal",
        }
    }
}

/// Startup-resolved copy policy: the only fact the event loop consults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyPolicy {
    /// Default target for bare copy actions.
    pub default_target: CopyDefaultTarget,
    /// Whether OSC 52 may emit bytes.
    pub allowed: bool,
    /// Why writes are denied (`None` when allowed).
    pub deny: Option<CopyDenyReason>,
}

impl CopyPolicy {
    /// Fully denied baseline: no bytes until the startup policy arrives.
    pub fn disabled() -> Self {
        Self {
            default_target: CopyDefaultTarget::Message,
            allowed: false,
            deny: Some(CopyDenyReason::Disabled),
        }
    }
    /// Resolve from the configured default, probed capabilities and the
    /// live TTY state, reading the strict environment override.
    pub fn resolve(
        default_target: CopyDefaultTarget,
        caps: &TerminalCapabilities,
        is_tty: bool,
    ) -> Self {
        Self::resolve_with_override(default_target, caps, is_tty, osc52_override())
    }

    /// Resolve with an explicit override value (test seam; production
    /// passes the environment readout).
    pub fn resolve_with_override(
        default_target: CopyDefaultTarget,
        caps: &TerminalCapabilities,
        is_tty: bool,
        override_value: Option<bool>,
    ) -> Self {
        if !is_tty {
            return Self {
                default_target,
                allowed: false,
                deny: Some(CopyDenyReason::NoTerminal),
            };
        }
        if override_value == Some(false) {
            return Self {
                default_target,
                allowed: false,
                deny: Some(CopyDenyReason::Disabled),
            };
        }
        if override_value == Some(true) {
            return Self {
                default_target,
                allowed: true,
                deny: None,
            };
        }
        if !caps.osc52_clipboard {
            return Self {
                default_target,
                allowed: false,
                deny: Some(CopyDenyReason::Disabled),
            };
        }
        Self {
            default_target,
            allowed: true,
            deny: None,
        }
    }

    /// User-facing notice when a copy cannot run.
    pub fn deny_notice(self) -> &'static str {
        self.deny.map_or("copy unavailable", CopyDenyReason::notice)
    }
}

impl Default for CopyPolicy {
    fn default() -> Self {
        Self::disabled()
    }
}

/// A copy request waiting for the post-frame flush point: the sanitized
/// payload is written after `draw` returns so the escape bytes never
/// interleave a differential frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCopy {
    pub text: String,
    pub label: String,
}

impl PendingCopy {
    pub fn new(text: String, label: impl Into<String>) -> Self {
        Self {
            text,
            label: label.into(),
        }
    }
}

/// Count lines and characters for the success notice, so a silently
/// truncated OSC 52 payload stays visible to the user.
pub fn count_lines_chars(text: &str) -> (usize, usize) {
    (text.lines().count().max(1), text.chars().count())
}

/// Success notice carrying the payload size.
pub fn success_notice(label: &str, text: &str) -> String {
    let (lines, chars) = count_lines_chars(text);
    format!("copied {label} ({lines} lines / {chars} chars)")
}

/// One scrollback row for assistant-segment extraction. The caller maps
/// its own line roles onto `is_assistant` so this crate never imports
/// transcript types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySourceLine {
    pub text: String,
    pub is_assistant: bool,
}

impl CopySourceLine {
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_assistant: true,
        }
    }

    pub fn other(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_assistant: false,
        }
    }
}

/// Extract the last contiguous assistant segment, joined by newlines.
/// Returns `None` when no assistant row exists.
pub fn last_assistant_text(lines: &[CopySourceLine]) -> Option<String> {
    let last = lines.iter().rposition(|line| line.is_assistant)?;
    let mut start = last;
    while start > 0 && lines[start - 1].is_assistant {
        start -= 1;
    }
    Some(
        lines[start..=last]
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Explicit copy target: the `message` / `code [N]` / `all` / `id` family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyTargetSpec {
    /// The whole last assistant message.
    Message,
    /// One fenced code block (`None` is the last block).
    Code { index: Option<u32> },
    /// Every fenced code block, joined by a blank line.
    All,
    /// The session / execution identifier.
    Id,
}

/// Map a bare `/copy` (no argument) onto the configured default.
pub fn spec_or_default(
    arg: &str,
    default: CopyDefaultTarget,
) -> Result<CopyTargetSpec, CopyResolveError> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        return Ok(match default {
            CopyDefaultTarget::Message => CopyTargetSpec::Message,
            CopyDefaultTarget::Code => CopyTargetSpec::Code { index: None },
        });
    }
    parse_explicit_spec(trimmed)
}

/// Parse an explicit `/copy` argument.
pub fn parse_explicit_spec(arg: &str) -> Result<CopyTargetSpec, CopyResolveError> {
    let trimmed = arg.trim();
    if trimmed.eq_ignore_ascii_case("message") || trimmed.eq_ignore_ascii_case("msg") {
        return Ok(CopyTargetSpec::Message);
    }
    if trimmed.eq_ignore_ascii_case("all") {
        return Ok(CopyTargetSpec::All);
    }
    if trimmed.eq_ignore_ascii_case("id") {
        return Ok(CopyTargetSpec::Id);
    }
    if trimmed.eq_ignore_ascii_case("code") {
        return Ok(CopyTargetSpec::Code { index: None });
    }
    let mut parts = trimmed.split_whitespace();
    if let Some(first) = parts.next() {
        if first.eq_ignore_ascii_case("code") {
            if let Some(num) = parts.next() {
                if parts.next().is_none() {
                    if let Ok(index) = num.parse::<u32>() {
                        return Ok(CopyTargetSpec::Code { index: Some(index) });
                    }
                }
            }
        }
    }
    Err(CopyResolveError::UnknownTarget)
}

/// Copy resolution failure, one variant per user-facing branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResolveError {
    /// No assistant row in the scrollback.
    NoAssistantMessage,
    /// The last message holds only whitespace.
    EmptyReply,
    /// The last message holds no fenced code blocks.
    NoCodeBlocks,
    /// `code N` outside `1..=total`.
    IndexOutOfRange { requested: u32, total: usize },
    /// Unparseable `/copy` argument.
    UnknownTarget,
    /// `id` with no session attached yet.
    NoSessionId,
}

impl CopyResolveError {
    /// User-facing notice for the failure.
    pub fn notice(&self) -> String {
        match self {
            CopyResolveError::NoAssistantMessage => "copy: no assistant message yet".to_string(),
            CopyResolveError::EmptyReply => "copy: last reply is empty".to_string(),
            CopyResolveError::NoCodeBlocks => {
                "copy: no code blocks in the last message".to_string()
            }
            CopyResolveError::IndexOutOfRange { requested, total } => {
                format!("copy: code block {requested} out of range (1..={total})")
            }
            CopyResolveError::UnknownTarget => {
                "copy: unknown target (message | code [N] | all | id)".to_string()
            }
            CopyResolveError::NoSessionId => "copy: no session id yet".to_string(),
        }
    }
}

/// Extract fenced code blocks from markdown: an opening fence starts a
/// block, its closing fence ends it, and a trailing unclosed fence still
/// counts so partial streams stay copyable.
pub fn extract_code_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            if let Some(body) = current.take() {
                blocks.push(body.join("\n"));
            } else {
                current = Some(Vec::new());
            }
        } else if let Some(body) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some(body) = current.take() {
        blocks.push(body.join("\n"));
    }
    blocks
}

/// Resolve a target against the already-extracted last message text and
/// session id. The caller owns scrollback extraction; this stays pure.
pub fn resolve_copy_text(
    spec: CopyTargetSpec,
    assistant_text: Option<&str>,
    session_id: Option<&str>,
) -> Result<(String, String), CopyResolveError> {
    match spec {
        CopyTargetSpec::Id => {
            let id = session_id.unwrap_or("").trim();
            if id.is_empty() {
                return Err(CopyResolveError::NoSessionId);
            }
            Ok((id.to_string(), "session id".to_string()))
        }
        CopyTargetSpec::Message => {
            let text = assistant_text.ok_or(CopyResolveError::NoAssistantMessage)?;
            if text.trim().is_empty() {
                return Err(CopyResolveError::EmptyReply);
            }
            Ok((text.to_string(), "message".to_string()))
        }
        CopyTargetSpec::Code { index } => {
            let text = assistant_text.ok_or(CopyResolveError::NoAssistantMessage)?;
            if text.trim().is_empty() {
                return Err(CopyResolveError::EmptyReply);
            }
            let blocks = extract_code_blocks(text);
            let total = blocks.len();
            if total == 0 {
                return Err(CopyResolveError::NoCodeBlocks);
            }
            match index {
                None => blocks
                    .into_iter()
                    .last()
                    .map_or(Err(CopyResolveError::NoCodeBlocks), |block| {
                        Ok((block, format!("code block {total}/{total}")))
                    }),
                Some(requested) => {
                    if requested == 0 || usize::try_from(requested).unwrap_or(usize::MAX) > total {
                        return Err(CopyResolveError::IndexOutOfRange { requested, total });
                    }
                    let idx = usize::try_from(requested).unwrap_or(1).saturating_sub(1);
                    Ok((
                        blocks[idx].clone(),
                        format!("code block {requested}/{total}"),
                    ))
                }
            }
        }
        CopyTargetSpec::All => {
            let text = assistant_text.ok_or(CopyResolveError::NoAssistantMessage)?;
            if text.trim().is_empty() {
                return Err(CopyResolveError::EmptyReply);
            }
            let blocks = extract_code_blocks(text);
            if blocks.is_empty() {
                return Err(CopyResolveError::NoCodeBlocks);
            }
            let total = blocks.len();
            Ok((blocks.join("\n\n"), format!("{total} code blocks")))
        }
    }
}

/// Copy a visual wrapped-row range back to source text.
///
/// `intervals` maps each wrapped row to its `(logical, col_start, col_end)`
/// source interval (see [`crate::wrap_map::WrappedColumnMap`]);
/// `source_lines` holds the logical source rows. Rows between the endpoints
/// copy whole; the endpoint rows clip to their covered columns so a
/// cross-row selection aligns exactly instead of duplicating wrapped text.
pub fn copy_wrapped_range(
    intervals: &[(usize, usize, usize)],
    source_lines: &[String],
    anchor: usize,
    focus: usize,
) -> Option<String> {
    let (from, to) = if anchor <= focus {
        (anchor, focus)
    } else {
        (focus, anchor)
    };
    let (first_logical, first_start, _) = *intervals.get(from)?;
    let (last_logical, _, last_end) = *intervals.get(to)?;
    if first_logical == last_logical {
        let text = source_lines.get(first_logical)?;
        return Some(crate::wrap_map::WrappedColumnMap::slice_columns(
            text,
            first_start,
            last_end,
        ));
    }
    let mut out = Vec::new();
    let first_text = source_lines.get(first_logical)?;
    out.push(crate::wrap_map::WrappedColumnMap::slice_columns(
        first_text,
        first_start,
        usize::MAX,
    ));
    let mut logical = first_logical + 1;
    while logical < last_logical {
        out.push(source_lines.get(logical)?.clone());
        logical += 1;
    }
    let last_text = source_lines.get(last_logical)?;
    out.push(crate::wrap_map::WrappedColumnMap::slice_columns(
        last_text,
        0,
        last_end,
    ));
    Some(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_with(osc52: bool) -> TerminalCapabilities {
        TerminalCapabilities {
            osc52_clipboard: osc52,
            ..TerminalCapabilities::default()
        }
    }

    #[test]
    fn policy_truth_table() {
        let capable = caps_with(true);
        let incapable = caps_with(false);
        let allowed =
            CopyPolicy::resolve_with_override(CopyDefaultTarget::Message, &capable, true, None);
        assert!(allowed.allowed);
        assert_eq!(allowed.deny, None);
        let denied =
            CopyPolicy::resolve_with_override(CopyDefaultTarget::Message, &incapable, true, None);
        assert!(!denied.allowed);
        assert_eq!(denied.deny, Some(CopyDenyReason::Disabled));
        let piped =
            CopyPolicy::resolve_with_override(CopyDefaultTarget::Message, &capable, false, None);
        assert!(!piped.allowed);
        assert_eq!(piped.deny, Some(CopyDenyReason::NoTerminal));
        let forced = CopyPolicy::resolve_with_override(
            CopyDefaultTarget::Code,
            &incapable,
            true,
            Some(true),
        );
        assert!(forced.allowed);
        assert_eq!(forced.default_target, CopyDefaultTarget::Code);
        let killed = CopyPolicy::resolve_with_override(
            CopyDefaultTarget::Message,
            &capable,
            true,
            Some(false),
        );
        assert!(!killed.allowed);
        assert_eq!(killed.deny, Some(CopyDenyReason::Disabled));
    }

    #[test]
    fn deny_and_success_notices() {
        assert!(CopyDenyReason::Disabled
            .notice()
            .contains("WF_TUI_OSC52_CLIPBOARD"));
        assert_eq!(
            success_notice("message", "a\nb\nc"),
            "copied message (3 lines / 5 chars)"
        );
    }

    #[test]
    fn last_segment_covers_the_trailing_assistant_run() {
        let lines = vec![
            CopySourceLine::assistant("first"),
            CopySourceLine::assistant("second"),
            CopySourceLine::other("tool row"),
            CopySourceLine::assistant("third"),
            CopySourceLine::assistant("fourth"),
        ];
        assert_eq!(
            last_assistant_text(&lines).as_deref(),
            Some("third\nfourth")
        );
        let none: Vec<CopySourceLine> =
            vec![CopySourceLine::other("x"), CopySourceLine::other("y")];
        assert_eq!(last_assistant_text(&none), None);
    }

    #[test]
    fn spec_parsing_covers_the_family() {
        assert_eq!(
            spec_or_default("", CopyDefaultTarget::Message),
            Ok(CopyTargetSpec::Message)
        );
        assert_eq!(
            spec_or_default("", CopyDefaultTarget::Code),
            Ok(CopyTargetSpec::Code { index: None })
        );
        assert_eq!(parse_explicit_spec("message"), Ok(CopyTargetSpec::Message));
        assert_eq!(parse_explicit_spec("msg"), Ok(CopyTargetSpec::Message));
        assert_eq!(
            parse_explicit_spec("code"),
            Ok(CopyTargetSpec::Code { index: None })
        );
        assert_eq!(
            parse_explicit_spec("code 2"),
            Ok(CopyTargetSpec::Code { index: Some(2) })
        );
        assert_eq!(parse_explicit_spec("all"), Ok(CopyTargetSpec::All));
        assert_eq!(parse_explicit_spec("id"), Ok(CopyTargetSpec::Id));
        assert_eq!(
            parse_explicit_spec("bogus"),
            Err(CopyResolveError::UnknownTarget)
        );
        assert_eq!(
            parse_explicit_spec("code nope"),
            Err(CopyResolveError::UnknownTarget)
        );
    }

    #[test]
    fn code_extraction_handles_fences() {
        let text = "intro\n```rust\nlet x = 1;\n```\nmiddle\n```\nplain\n```\n";
        let blocks = extract_code_blocks(text);
        assert_eq!(blocks, vec!["let x = 1;".to_string(), "plain".to_string()]);
        let unclosed = "hi\n```py\nprint(1)\n";
        assert_eq!(extract_code_blocks(unclosed), vec!["print(1)".to_string()]);
        assert!(extract_code_blocks("no fences").is_empty());
    }

    #[test]
    fn resolve_enumerates_targets_and_failures() {
        let text = "see:\n```rust\nlet a = 1;\n```\nand:\n```\nb\n```\n";
        let (body, label) =
            resolve_copy_text(CopyTargetSpec::Message, Some(text), None).expect("message");
        assert_eq!(body, text);
        assert_eq!(label, "message");
        let (body, label) =
            resolve_copy_text(CopyTargetSpec::Code { index: None }, Some(text), None)
                .expect("last block");
        assert_eq!(body, "b");
        assert_eq!(label, "code block 2/2");
        let (body, _) =
            resolve_copy_text(CopyTargetSpec::Code { index: Some(1) }, Some(text), None)
                .expect("first block");
        assert_eq!(body, "let a = 1;");
        let (body, label) = resolve_copy_text(CopyTargetSpec::All, Some(text), None).expect("all");
        assert_eq!(body, "let a = 1;\n\nb");
        assert_eq!(label, "2 code blocks");
        let (body, label) =
            resolve_copy_text(CopyTargetSpec::Id, Some(text), Some("sess-9")).expect("id");
        assert_eq!(body, "sess-9");
        assert_eq!(label, "session id");

        assert_eq!(
            resolve_copy_text(CopyTargetSpec::Message, None, None),
            Err(CopyResolveError::NoAssistantMessage)
        );
        assert_eq!(
            resolve_copy_text(CopyTargetSpec::Message, Some("  \n "), None),
            Err(CopyResolveError::EmptyReply)
        );
        assert_eq!(
            resolve_copy_text(CopyTargetSpec::Code { index: None }, Some("plain"), None),
            Err(CopyResolveError::NoCodeBlocks)
        );
        assert_eq!(
            resolve_copy_text(CopyTargetSpec::Code { index: Some(5) }, Some(text), None),
            Err(CopyResolveError::IndexOutOfRange {
                requested: 5,
                total: 2
            })
        );
        assert_eq!(
            resolve_copy_text(CopyTargetSpec::Id, Some(text), None),
            Err(CopyResolveError::NoSessionId)
        );
    }
}
