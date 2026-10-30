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
