//! Construction helper for `LlmRequest`.
//!
//! The protocol struct carries a wide tail of optional fields and no
//! `Default` implementation, so every caller used to repeat all of them just
//! to set `profile_id` and `messages`. New protocol fields then had to be
//! threaded through every construction site. This helper gives a single
//! default-carrying entry point; callers override the few fields their path
//! actually needs.

use llm_types::llm::LlmRequest;
use llm_types::message::Message;

/// Build a request with `profile_id` and `messages` set and every optional
/// field left at its neutral value.
///
/// Callers that need tools, a protocol, timeout or any other non-default field
/// must set it explicitly on the returned value. The helper never infers those
/// from the profile: a tool-free chat request and a tool-carrying agent request
/// are intentionally different paths.
pub fn new_llm_request(profile_id: impl Into<String>, messages: Vec<Message>) -> LlmRequest {
    LlmRequest {
        profile_id: profile_id.into(),
        messages,
        parameters: None,
        generation: None,
        tools: None,
        tool_call_protocol: None,
        locked_tool_call_protocol: None,
        violation_policy: None,
        execution_id: None,
        stream: None,
        dead_loop_detection: None,
        protocol_auto_converted: None,
        timeout_ms: None,
        prompt_caching: None,
        cached_content: None,
        max_retries: None,
        retry_delay_ms: None,
        extra_headers: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_leaves_optionals_neutral() {
        let request = new_llm_request("p1", vec![Message::user_text("hi".to_string())]);
        assert_eq!(request.profile_id, "p1");
        assert_eq!(request.messages.len(), 1);
        assert!(request.tools.is_none());
        assert!(request.execution_id.is_none());
        assert!(request.timeout_ms.is_none());
    }
}
