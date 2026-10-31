use wf_execution_shared::hooks::HookContext;
use wf_types::message::Message;

/// Parsed payload of one `CONTEXT_COMPRESSION_REQUESTED` hook signal.
pub(super) struct CompressionSignal {
    pub(super) target_context_id: String,
    pub(super) tokens_used: u64,
    pub(super) token_limit: u64,
    pub(super) message_count: usize,
    pub(super) array_version: u64,
    pub(super) forced: bool,
    pub(super) depth: u64,
    pub(super) messages: Vec<Message>,
    /// Compaction work plan; empty means the legacy single-summary mode.
    /// Malformed plans degrade to empty rather than failing the signal.
    pub(super) plan: Vec<wf_execution_shared::CompactionPlanItem>,
    /// Present when the emitting execution is an agent loop (its conversation
    /// self-consumes the completed event); absent for workflow targets.
    pub(super) agent_loop_id: Option<String>,
}

/// Parse the compression signal payload from a hook context; `None` when the
/// payload is missing or invalid (logged skip, never a fire failure). The
/// snapshot and plan come from the process-local attachment when present
/// (the emitter path, no JSON round-trip); the JSON hook data stays as the
/// fallback for foreign emitters that build the context by hand.
pub(super) fn parse_compression_signal(ctx: &HookContext) -> Option<CompressionSignal> {
    use wf_execution_shared::token_events::{
        KEY_ARRAY_VERSION, KEY_FORCED, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID,
        KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    };
    let get = |key: &str| ctx.data.get(key);
    let (messages, plan) = match ctx.attachment.as_ref() {
        Some(attachment) => (
            attachment.messages.as_ref().clone(),
            attachment.plan.as_ref().clone(),
        ),
        None => (
            get(KEY_MESSAGES)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            get(wf_execution_shared::KEY_PLAN)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        ),
    };
    Some(CompressionSignal {
        target_context_id: get(KEY_TARGET_CONTEXT_ID)?.as_str()?.to_string(),
        tokens_used: get(KEY_TOKENS_USED)?.as_u64()?,
        token_limit: get(KEY_TOKEN_LIMIT)?.as_u64()?,
        message_count: get(KEY_MESSAGE_COUNT)?.as_u64()? as usize,
        array_version: get(KEY_ARRAY_VERSION).and_then(|v| v.as_u64()).unwrap_or(0),
        forced: get(KEY_FORCED).and_then(|v| v.as_bool()).unwrap_or(false),
        depth: get(wf_execution_shared::KEY_COMPRESSION_DEPTH)
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        messages,
        plan,
        agent_loop_id: ctx
            .data
            .get("agent_loop_id")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use wf_execution_shared::token_events::{
        CompressionAttachment, KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_MESSAGE_COUNT,
        KEY_TARGET_CONTEXT_ID, KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
    };

    fn text_message(text: &str) -> Message {
        Message::user_text(text.to_string())
    }

    fn audit_data(message_count: u64) -> HashMap<String, serde_json::Value> {
        let mut data = HashMap::new();
        data.insert(KEY_TARGET_CONTEXT_ID.to_string(), "chat".into());
        data.insert(KEY_TOKENS_USED.to_string(), 900u64.into());
        data.insert(KEY_TOKEN_LIMIT.to_string(), 1000u64.into());
        data.insert(KEY_MESSAGE_COUNT.to_string(), message_count.into());
        data.insert(KEY_ARRAY_VERSION.to_string(), 7u64.into());
        data
    }

    fn hook_context(data: HashMap<String, serde_json::Value>) -> HookContext {
        HookContext::new(
            wf_types::Id::from(wf_common::generate_id()),
            wf_execution_shared::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
            data,
            tokio_util::sync::CancellationToken::new(),
        )
    }

    #[test]
    fn attachment_wins_over_hook_data() {
        let attached = vec![text_message("attached")];
        let mut data = audit_data(1);
        data.insert(
            KEY_MESSAGES.to_string(),
            serde_json::to_value(vec![text_message("stale json")]).unwrap(),
        );
        let ctx = hook_context(data).with_attachment(CompressionAttachment {
            messages: Arc::new(attached),
            plan: Arc::new(Vec::new()),
        });
        let signal = parse_compression_signal(&ctx).expect("signal parses");
        assert_eq!(signal.messages.len(), 1);
        assert_eq!(
            signal.messages[0].content,
            wf_types::message::MessageContentValue::Text("attached".to_string())
        );
    }

    #[test]
    fn json_data_stays_a_fallback() {
        let messages = vec![text_message("json fallback")];
        let mut data = audit_data(1);
        data.insert(
            KEY_MESSAGES.to_string(),
            serde_json::to_value(&messages).unwrap(),
        );
        let ctx = hook_context(data);
        let signal = parse_compression_signal(&ctx).expect("signal parses");
        assert_eq!(signal.messages.len(), 1);
    }
}
