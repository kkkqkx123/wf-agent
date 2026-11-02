use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Upper bound of text attachments carried by one turn.
/// Mirrors the frontend composer budget so both sides reject the same
/// oversized stages locally instead of failing only at the server gate.
///
/// Counted in Unicode scalar values to match the frontend `text.length`
/// truncation point closely enough for a budget gate; exact byte
/// accounting happens per provider codec downstream.
pub const MAX_TEXT_ATTACHMENTS_PER_TURN: usize = 8;

/// Upper bound of characters kept per text attachment (64K). Larger files
/// must arrive truncated with an explicit marker and original size.
pub const MAX_TEXT_ATTACHMENT_CHARS: usize = 64_000;

/// Upper bound of staged attachment characters across one turn (256K).
pub const MAX_TEXT_ATTACHMENTS_TOTAL_CHARS: usize = 256_000;

/// One text file attached to a user turn. Transport carries the possibly
/// truncated `content` plus an explicit marker and original size so the
/// server never mistakes truncated content for the full file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserTextAttachment {
    /// Original filename for display and retry.
    pub name: String,
    /// Possibly truncated file text, already validated for control chars.
    pub content: String,
    /// True when `content` was cut to the per-file limit.
    #[serde(default)]
    pub truncated: bool,
    /// Original character count before truncation. Present when truncated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_chars: Option<usize>,
}

impl UserTextAttachment {
    /// Whether the entry carries any usable payload.
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty() && self.content.is_empty()
    }
}

/// Validate one turn of text attachments. Every problem is a hard error:
/// oversized or over-count stages must never degrade into silent drops.
pub fn validate_user_attachments(attachments: &[UserTextAttachment]) -> Result<(), String> {
    if attachments.len() > MAX_TEXT_ATTACHMENTS_PER_TURN {
        return Err(format!(
            "too many attachments: {} exceed the {MAX_TEXT_ATTACHMENTS_PER_TURN}-file per-turn limit",
            attachments.len()
        ));
    }
    let mut total = 0usize;
    for (index, attachment) in attachments.iter().enumerate() {
        let field = format!("attachments[{index}]");
        if attachment.name.trim().is_empty() {
            return Err(format!("{field} needs a filename"));
        }
        let chars = attachment.content.chars().count();
        if chars > MAX_TEXT_ATTACHMENT_CHARS && !attachment.truncated {
            return Err(format!(
                "{field} '{}' holds {chars} chars, exceeding the per-file limit of {MAX_TEXT_ATTACHMENT_CHARS} chars without a truncation marker",
                attachment.name.trim()
            ));
        }
        if chars > MAX_TEXT_ATTACHMENT_CHARS
            && attachment.truncated
            && attachment
                .original_chars
                .is_none_or(|n| n <= MAX_TEXT_ATTACHMENT_CHARS)
        {
            return Err(format!(
                "{field} '{}' is marked truncated but carries no larger original size",
                attachment.name.trim()
            ));
        }
        if attachment.content.contains('\0') {
            return Err(format!(
                "{field} '{}' looks like a binary file; send images via the image channel instead",
                attachment.name.trim()
            ));
        }
        total += chars;
    }
    if total > MAX_TEXT_ATTACHMENTS_TOTAL_CHARS {
        return Err(format!(
            "attachments hold {total} chars in total, exceeding the per-turn limit of {MAX_TEXT_ATTACHMENTS_TOTAL_CHARS} chars"
        ));
    }
    Ok(())
}

/// Escape an attachment filename for the inline block header. Mirrors the
/// frontend `escapeAttachmentName` so both sides produce identical wire.
fn escape_attachment_name(name: &str) -> String {
    name.replace('&', "&amp;").replace('"', "&quot;")
}

/// Escape attachment content for the inline block body. Mirrors the frontend
/// `escapeAttachmentContent` so nested markers never split on either side.
fn escape_attachment_content(content: &str) -> String {
    content
        .replace('\\', "\\\\")
        .replace("<attachment ", "<\\attachment ")
        .replace("</attachment>", "<\\/attachment>")
}

/// Format structured attachments into inline blocks with truncation
/// attributes for storage and the model prompt. The frontend sends raw text
/// plus structured attachments; the server re-inlines them here so history
/// stays a single user bubble and old readers that ignore attributes keep
/// working. Truncation is explicit in the wire so the model never mistakes
/// cut content for the full file.
pub fn format_attachments_for_storage(text: &str, attachments: &[UserTextAttachment]) -> String {
    if attachments.is_empty() {
        return text.to_string();
    }
    let mut blocks = Vec::with_capacity(attachments.len());
    for file in attachments {
        let mut attrs = String::new();
        if file.truncated {
            attrs.push_str(" truncated=\"true\"");
        }
        if let Some(original) = file.original_chars {
            attrs.push_str(&format!(" original=\"{original}\""));
        }
        blocks.push(format!(
            "<attachment name=\"{}\"{attrs}>\n{}\n</attachment>",
            escape_attachment_name(&file.name),
            escape_attachment_content(&file.content)
        ));
    }
    if text.is_empty() {
        return blocks.join("\n\n");
    }
    format!("{text}\n\n{}", blocks.join("\n\n"))
}

/// Upper bound for an imported conversation seeding a new turn.
pub const MAX_IMPORTED_CONVERSATION_MESSAGES: usize = 4096;

/// Whether user-supplied text carries no usable content.
pub fn is_blank_user_text(text: &str) -> bool {
    text.trim().is_empty()
}

/// Strip ANSI CSI escape sequences from user-provided text, preserving
/// `\n` and `\t`. Single owner for input cleaning so every server entry
/// washes identically; clients pass raw text through and never wash.
pub fn sanitize_user_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < len && (bytes[j] & 0xe0 == 0x20 || bytes[j] & 0xf0 == 0x30) {
                j += 1;
            }
            if j < len && bytes[j] & 0x80 == 0 && bytes[j] >= 0x40 {
                j += 1;
            }
            i = j;
            continue;
        }
        let b = bytes[i];
        if b == b'\n' || b == b'\t' || b >= 0x20 || b == 0x1b {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

/// Non-empty user text. Construction rejects blank input so holders never
/// re-check emptiness downstream. The server gate stays authoritative;
/// clients use the same predicate for early failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMessage(String);

impl UserMessage {
    pub fn try_new(text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        if is_blank_user_text(&text) {
            return Err("agent message must not be empty".to_string());
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl Serialize for UserMessage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for UserMessage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        UserMessage::try_new(text).map_err(serde::de::Error::custom)
    }
}

/// One imported-conversation shape problem with a structured reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationIssue {
    pub field: String,
    pub message: String,
}

impl ConversationIssue {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Shape check for an imported conversation: length bound plus basic role
/// legitimacy. Roles are already constrained by the message type; the extra
/// rule is that tool messages must carry their correlation id, otherwise
/// the imported history cannot be replayed. Empty-text messages without
/// tool calls are reported here instead of being silently dropped.
pub fn validate_imported_conversation(
    messages: &[crate::message::Message],
) -> Vec<ConversationIssue> {
    let mut issues = Vec::new();
    if messages.len() > MAX_IMPORTED_CONVERSATION_MESSAGES {
        issues.push(ConversationIssue::new(
            "conversation",
            format!(
                "conversation holds {} messages, exceeding the {}-message import limit",
                messages.len(),
                MAX_IMPORTED_CONVERSATION_MESSAGES
            ),
        ));
    }
    for (index, message) in messages.iter().enumerate() {
        if matches!(message.role, crate::message::MessageRole::Tool)
            && message
                .tool_call_id
                .as_ref()
                .is_none_or(|id| id.trim().is_empty())
        {
            issues.push(ConversationIssue::new(
                format!("conversation[{index}]"),
                "tool message must carry its tool call id".to_string(),
            ));
        }
        let has_text = !message.text_content().trim().is_empty();
        let has_tool_calls = message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty());
        if !has_text && !has_tool_calls {
            issues.push(ConversationIssue::new(
                format!("conversation[{index}]"),
                "message carries neither text nor tool calls".to_string(),
            ));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_text_is_rejected_everywhere() {
        assert!(is_blank_user_text("   "));
        assert!(!is_blank_user_text("hi"));
        assert!(UserMessage::try_new("   ").is_err());
        assert_eq!(UserMessage::try_new("hi").expect("valid").as_str(), "hi");
    }

    #[test]
    fn sanitize_strips_csi_but_keeps_text_layout() {
        assert_eq!(sanitize_user_text("hello world"), "hello world");
        assert_eq!(sanitize_user_text("line1\nline2\tend"), "line1\nline2\tend");
        assert_eq!(sanitize_user_text("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(sanitize_user_text("\x1b[2J\x1b[H"), "");
        assert_eq!(sanitize_user_text("a\x07b"), "ab");
        assert_eq!(sanitize_user_text(""), "");
    }

    fn attachment(name: &str, content: &str) -> UserTextAttachment {
        UserTextAttachment {
            name: name.to_string(),
            content: content.to_string(),
            truncated: false,
            original_chars: None,
        }
    }

    #[test]
    fn attachment_limits_reject_over_count_and_binary() {
        let many = (0..MAX_TEXT_ATTACHMENTS_PER_TURN + 1)
            .map(|i| attachment(&format!("f{i}.txt"), "x"))
            .collect::<Vec<_>>();
        assert!(validate_user_attachments(&many).is_err());
        let binary = UserTextAttachment {
            name: "bin".to_string(),
            content: "a\0b".to_string(),
            truncated: false,
            original_chars: None,
        };
        assert!(validate_user_attachments(&[binary]).is_err());
    }

    #[test]
    fn attachment_truncation_needs_marker_and_original() {
        let big = "x".repeat(MAX_TEXT_ATTACHMENT_CHARS + 1);
        assert!(validate_user_attachments(&[attachment("a.txt", &big)]).is_err());
        let marked = UserTextAttachment {
            name: "a.txt".to_string(),
            content: "x".repeat(MAX_TEXT_ATTACHMENT_CHARS),
            truncated: true,
            original_chars: Some(MAX_TEXT_ATTACHMENT_CHARS + 10),
        };
        assert!(validate_user_attachments(&[marked]).is_ok());
        let missing_original = UserTextAttachment {
            name: "a.txt".to_string(),
            content: "x".repeat(MAX_TEXT_ATTACHMENT_CHARS + 1),
            truncated: true,
            original_chars: None,
        };
        assert!(validate_user_attachments(&[missing_original]).is_err());
    }

    #[test]
    fn attachment_storage_format_carries_truncation_attributes() {
        let files = vec![UserTextAttachment {
            name: "a.txt".to_string(),
            content: "hello".to_string(),
            truncated: true,
            original_chars: Some(99999),
        }];
        let wire = format_attachments_for_storage("see", &files);
        assert!(wire.contains("truncated=\"true\""));
        assert!(wire.contains("original=\"99999\""));
        assert!(format_attachments_for_storage("", &[]).is_empty());
    }
}
