use std::collections::HashMap;

use wf_types::message::{Message, MessageContentValue, MessageRole};

/// Cross-boundary direction for request-time message adaptation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryDirection {
    WorkflowToAgent,
    AgentToWorkflow,
}

/// Adapt messages for a cross-boundary request without mutating stored
/// history. Workflow-to-agent drops native tool-call fields (the agent
/// re-derives them from its own protocol); agent-to-workflow drops
/// system and tool messages that have no workflow meaning.
/// Pure request-assembly helper: inputs are only cloned.
pub fn convert_for_boundary(messages: &[Message], direction: BoundaryDirection) -> Vec<Message> {
    match direction {
        BoundaryDirection::WorkflowToAgent => messages
            .iter()
            .map(|msg| {
                let mut converted = msg.clone();
                converted.tool_calls = None;
                converted
            })
            .collect(),
        BoundaryDirection::AgentToWorkflow => messages
            .iter()
            .filter(|msg| !matches!(msg.role, MessageRole::System | MessageRole::Tool))
            .cloned()
            .collect(),
    }
}

/// Prepend workflow context variables as a system message for one request.
/// Returns a new vector; the stored history is untouched. Empty context
/// returns the input unchanged.
pub fn inject_context(
    messages: &[Message],
    context_vars: &HashMap<String, String>,
) -> Vec<Message> {
    if context_vars.is_empty() {
        return messages.to_vec();
    }
    let mut lines = String::from("Current context variables:\n");
    for (key, value) in context_vars {
        lines.push_str(&format!("  {key} = {value}\n"));
    }
    let context_message = Message {
        id: wf_common::generate_id(),
        role: MessageRole::System,
        content: MessageContentValue::Text(lines),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    };
    let mut out = Vec::with_capacity(messages.len() + 1);
    out.push(context_message);
    out.extend(messages.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_message(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn workflow_to_agent_strips_tool_calls() {
        let mut assistant = text_message(MessageRole::Assistant, "checking");
        assistant.tool_calls = Some(vec![wf_types::message::LlmToolCall {
            id: "c1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "search".to_string(),
                arguments: "{}".to_string(),
            },
        }]);
        let converted =
            convert_for_boundary(&[assistant.clone()], BoundaryDirection::WorkflowToAgent);
        assert!(converted[0].tool_calls.is_none());
        assert!(assistant.tool_calls.is_some());
    }

    #[test]
    fn agent_to_workflow_drops_system_and_tool() {
        let history = vec![
            text_message(MessageRole::System, "sys"),
            text_message(MessageRole::User, "hi"),
            text_message(MessageRole::Tool, "result"),
        ];
        let converted = convert_for_boundary(&history, BoundaryDirection::AgentToWorkflow);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, MessageRole::User);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn empty_context_returns_input_unchanged() {
        let history = vec![text_message(MessageRole::User, "hi")];
        let converted = inject_context(&history, &HashMap::new());
        assert_eq!(converted, history);
    }
}
