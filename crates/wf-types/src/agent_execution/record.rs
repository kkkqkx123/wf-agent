use serde::{Deserialize, Serialize};

use crate::Timestamp;

/// Default preview length for LLM request/response summaries.
///
/// Oversized fields are truncated to this many characters; the caller sets
/// the `truncated` marker so the audit consumer knows the preview is lossy
/// (`truncation_stats`).
pub const LLM_SUMMARY_PREVIEW_MAX: usize = 512;

/// Truncate a string to the summary preview limit, returning the trimmed
/// text and whether truncation happened.
pub fn truncate_summary_preview(text: &str) -> (String, bool) {
    if text.len() <= LLM_SUMMARY_PREVIEW_MAX {
        return (text.to_string(), false);
    }
    let mut end = LLM_SUMMARY_PREVIEW_MAX;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}…", &text[..end]), true)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IterationRecord {
    pub iteration: u32,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
    /// LLM requests issued by this iteration (audit trail). Absent in
    /// older records; new fields deserialize cleanly from old blobs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_calls: Option<Vec<LlmCallRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Preview of one message embedded in an LLM request summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmMessageSummary {
    pub role: String,
    /// Truncated text preview of the message content.
    pub preview: String,
    /// Set when the preview was truncated to the preview limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

/// LLM request payload summary: message count, first/last message
/// previews, tool count and parameter entry count. Oversized fields are
/// truncated with the `truncated` marker set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmRequestSummary {
    pub message_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_message: Option<LlmMessageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<LlmMessageSummary>,
    pub tool_count: u32,
    pub parameter_count: u32,
    /// Set when any embedded preview was truncated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

/// LLM response summary: content preview and tool call count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmResponseSummary {
    /// Truncated text preview of the assistant content (absent when the
    /// response carried no text, e.g. pure tool-call replies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    /// Set when the content preview was truncated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    pub tool_call_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// One LLM call issued by an agent iteration (audit trail).
///
/// The record carries request/response summaries instead of full payloads:
/// it answers "what did the model receive and return" for audit without
/// duplicating the conversation into the checkpoint blob.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmCallRecord {
    /// 0-based sequence number within the owning iteration.
    pub seq: u32,
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_summary: Option<LlmRequestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_summary: Option<LlmResponseSummary>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub started_at: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<Timestamp>,
    pub duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_truncates_long_text() {
        let long = "x".repeat(LLM_SUMMARY_PREVIEW_MAX + 100);
        let (preview, truncated) = truncate_summary_preview(&long);
        assert!(truncated);
        assert!(
            preview.len() <= LLM_SUMMARY_PREVIEW_MAX + "…".len(),
            "preview length {} exceeds limit",
            preview.len()
        );
        assert!(preview.ends_with('…'));

        let short = "short".to_string();
        let (preview, truncated) = truncate_summary_preview(&short);
        assert_eq!(preview, "short");
        assert!(!truncated);
    }

    #[test]
    fn llm_call_record_round_trip() {
        let record = LlmCallRecord {
            seq: 0,
            profile_id: "p1".to_string(),
            model: Some("mock".to_string()),
            request_summary: Some(LlmRequestSummary {
                message_count: 2,
                first_message: Some(LlmMessageSummary {
                    role: "user".to_string(),
                    preview: "hi".to_string(),
                    truncated: None,
                }),
                last_message: None,
                tool_count: 1,
                parameter_count: 0,
                truncated: None,
            }),
            response_summary: Some(LlmResponseSummary {
                content_preview: Some("ok".to_string()),
                truncated: None,
                tool_call_count: 1,
                finish_reason: Some("tool_calls".to_string()),
            }),
            prompt_tokens: 10,
            completion_tokens: 20,
            started_at: 100,
            completed_at: Some(200),
            duration_ms: 100,
            error: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        let restored: LlmCallRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, record);
    }

    #[test]
    fn iteration_record_deserializes_old_blob_without_llm_calls() {
        let old = r#"{
            "iteration": 3,
            "started_at": 1000,
            "completed_at": 2000,
            "tool_calls": [{"id":"t1","name":"read","arguments":{},"started_at":1000,"completed_at":2000}],
            "response_content": "done"
        }"#;
        let record: IterationRecord = serde_json::from_str(old).unwrap();
        assert_eq!(record.iteration, 3);
        assert!(record.llm_calls.is_none());
        assert_eq!(record.tool_calls.as_ref().unwrap().len(), 1);
    }
}
