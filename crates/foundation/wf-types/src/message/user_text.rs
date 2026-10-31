use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Upper bound for an imported conversation seeding a new turn.
pub const MAX_IMPORTED_CONVERSATION_MESSAGES: usize = 4096;

/// Whether user-supplied text carries no usable content.
pub fn is_blank_user_text(text: &str) -> bool {
    text.trim().is_empty()
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
        if matches!(
            message.role,
            crate::message::MessageRole::Tool
        ) && message
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
        assert_eq!(
            UserMessage::try_new("hi").expect("valid").as_str(),
            "hi"
        );
    }
}
