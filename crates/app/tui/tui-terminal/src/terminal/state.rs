//! Terminal-side state display helpers: window title sanitization, OSC 9;4
//! progress sequences and the OSC 7501 program status protocol.

use base64::Engine as _;

/// Program status state reported to terminals that support OSC 7501.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusState {
    Idle,
    Working,
    Blocked,
    Done,
    Error,
    Clear,
}

impl ProgramStatusState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Error => "error",
            Self::Clear => "clear",
        }
    }
}

/// What a blocked program waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusKind {
    Permission,
    Question,
    Auth,
}

impl ProgramStatusKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Question => "question",
            Self::Auth => "auth",
        }
    }
}

/// Program status report (OSC 7501).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatus {
    pub state: ProgramStatusState,
    pub app: Option<String>,
    pub kind: Option<ProgramStatusKind>,
    pub message: Option<String>,
}

impl ProgramStatus {
    pub fn clear() -> Self {
        Self {
            state: ProgramStatusState::Clear,
            app: None,
            kind: None,
            message: None,
        }
    }
}

/// Feature detection query for the OSC 7501 protocol. A supporting
/// terminal replies with the same body, which [`is_program_status_reply`]
/// recognizes. Later spec revisions may append pairs after the `?`.
pub const PROGRAM_STATUS_QUERY: &str = "\x1b]7501;?\x1b\\";

/// Environment switch forcing program status reporting on or off.
/// Mirrors the override semantics: an explicit on value skips the live
/// probe and an explicit off value never emits.
pub const PROGRAM_STATUS_ENV: &str = "WF_TUI_PROGRAM_STATUS";

/// Explicit program status override from the environment: on for the
/// truthy spellings, off for the falsy spellings, undecided otherwise so
/// the caller falls back to the live terminal probe.
pub fn program_status_override() -> Option<bool> {
    match std::env::var(PROGRAM_STATUS_ENV).ok().as_deref() {
        Some("1" | "true" | "TRUE" | "yes") => Some(true),
        Some("0" | "false" | "FALSE" | "no") => Some(false),
        _ => None,
    }
}

/// Whether an input sequence is the reply to [`PROGRAM_STATUS_QUERY`].
/// Accepts either terminator and tolerates future pairs after the `?`.
pub fn is_program_status_reply(sequence: &str) -> bool {
    let Some(rest) = sequence.strip_prefix("\x1b]7501;?") else {
        return false;
    };
    let body = if let Some(body) = rest.strip_suffix("\x1b\\") {
        body
    } else if let Some(body) = rest.strip_suffix('\x07') {
        body
    } else {
        return false;
    };
    !body.contains('\x07') && !body.contains('\x1b')
}

const MAX_MESSAGE_BYTES: usize = 2048;

fn app_valid(app: &str) -> bool {
    (1..=32).contains(&app.len())
        && app
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-'))
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut bytes = 0usize;
    let mut end = 0usize;
    for ch in text.chars() {
        let size = ch.len_utf8();
        if bytes + size > max_bytes {
            break;
        }
        bytes += size;
        end += ch.len_utf8();
    }
    text[..end].to_string()
}

fn replace_control_chars(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Encode a status report as an OSC 7501 sequence.
pub fn format_program_status(status: &ProgramStatus) -> String {
    let mut pairs = vec![format!("state={}", status.state.as_str())];
    if let Some(app) = &status.app {
        if app_valid(app) {
            pairs.push(format!("app={app}"));
        }
    }
    if status.state == ProgramStatusState::Blocked {
        if let Some(kind) = status.kind {
            pairs.push(format!("kind={}", kind.as_str()));
        }
    }
    let raw_message = status.message.clone().unwrap_or_default();
    let message = truncate_utf8(
        replace_control_chars(&raw_message).trim(),
        MAX_MESSAGE_BYTES,
    );
    if !message.is_empty() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(message.as_bytes());
        pairs.push(format!("msg={encoded}"));
    }
    format!("\x1b]7501;{}\x1b\\", pairs.join(":"))
}

/// OSC sequences for the indeterminate progress indicator (OSC 9;4).
pub const PROGRESS_ACTIVE: &str = "\x1b]9;4;3\x07";
pub const PROGRESS_CLEAR: &str = "\x1b]9;4;0\x07";

/// Sanitize a terminal window title: drop control characters that could
/// terminate the OSC sequence early.
pub fn sanitize_title(title: &str) -> String {
    title.chars().filter(|c| !c.is_control()).collect()
}

/// Heartbeat frame glyphs for the window title while a turn runs. ASCII-safe
/// geometric shapes render consistently across common fonts, unlike braille
/// or emoji.
pub const RUNNING_TITLE_FRAMES: [&str; 2] = ["●", "○"];

/// Build a window title for one heartbeat tick. While running, the base
/// title is prefixed with the alternating frame glyph; when idle the base
/// title is returned unchanged. A frozen frame therefore signals a stalled
/// process.
pub fn running_title(base: &str, frame: usize, running: bool) -> String {
    if running {
        format!(
            "{} {base}",
            RUNNING_TITLE_FRAMES[frame % RUNNING_TITLE_FRAMES.len()]
        )
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_accepts_query_echo_with_either_terminator() {
        assert!(is_program_status_reply("\x1b]7501;?\x1b\\"));
        assert!(is_program_status_reply("\x1b]7501;?\x07"));
    }

    #[test]
    fn reply_tolerates_future_pairs() {
        assert!(is_program_status_reply("\x1b]7501;?version=2\x1b\\"));
    }

    #[test]
    fn override_reads_explicit_switches() {
        let saved = std::env::var(super::PROGRAM_STATUS_ENV).ok();
        let restore = || match saved.clone() {
            Some(value) => std::env::set_var(super::PROGRAM_STATUS_ENV, value),
            None => std::env::remove_var(super::PROGRAM_STATUS_ENV),
        };
        std::env::set_var(super::PROGRAM_STATUS_ENV, "1");
        assert_eq!(super::program_status_override(), Some(true));
        std::env::set_var(super::PROGRAM_STATUS_ENV, "0");
        assert_eq!(super::program_status_override(), Some(false));
        std::env::set_var(super::PROGRAM_STATUS_ENV, "false");
        assert_eq!(super::program_status_override(), Some(false));
        std::env::remove_var(super::PROGRAM_STATUS_ENV);
        assert_eq!(super::program_status_override(), None);
        std::env::set_var(super::PROGRAM_STATUS_ENV, "banana");
        assert_eq!(super::program_status_override(), None);
        restore();
    }

    #[test]
    fn reply_rejects_other_sequences() {
        assert!(!is_program_status_reply("\x1b]7501;state=idle\x1b\\"));
        assert!(!is_program_status_reply("\x1b]11;rgb:0000/0000/0000\x07"));
        assert!(!is_program_status_reply("\x1b]7501;?\x1b\\extra"));
        assert!(!is_program_status_reply("plain text"));
    }
}
