use wf_types::message::{Message, MessageContentValue, MessageRole};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryType {
    WorkflowToAgent,
    AgentToWorkflow,
    ParentToChild,
    ChildToParent,
}

pub struct CrossBoundaryConverter;

impl CrossBoundaryConverter {
    pub fn convert(messages: &[Message], from: &BoundaryType, to: &BoundaryType) -> Vec<Message> {
        match (from, to) {
            (BoundaryType::WorkflowToAgent, BoundaryType::AgentToWorkflow) => {
                Self::workflow_to_agent_to_workflow(messages)
            }
            (BoundaryType::AgentToWorkflow, BoundaryType::WorkflowToAgent) => {
                Self::agent_to_workflow_to_agent(messages)
            }
            _ => messages.to_vec(),
        }
    }

    fn workflow_to_agent_to_workflow(messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .map(|msg| {
                let mut converted = msg.clone();
                converted.tool_calls = None;
                converted
            })
            .collect()
    }

    fn agent_to_workflow_to_agent(messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .filter_map(|msg| match msg.role {
                MessageRole::System => None,
                MessageRole::Tool => None,
                _ => Some(msg.clone()),
            })
            .collect()
    }

    pub fn inject_context(
        messages: &[Message],
        context_vars: &std::collections::HashMap<String, String>,
    ) -> Vec<Message> {
        if context_vars.is_empty() {
            return messages.to_vec();
        }

        let mut result = messages.to_vec();

        let mut context_text = String::from("Current context variables:\n");
        for (key, value) in context_vars {
            context_text.push_str(&format!("  {} = {}\n", key, value));
        }

        let context_msg = Message {
            id: wf_types::Id::new(),
            role: MessageRole::System,
            content: MessageContentValue::Text(context_text),
            timestamp: wf_common::time::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };

        result.insert(0, context_msg);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_to_agent_strips_tool_calls() {
        let messages = vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text("test".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: Some(vec![wf_types::message::LlmToolCall {
                id: "tc1".to_string(),
                r#type: "function".to_string(),
                function: wf_types::message::LlmFunctionCall {
                    name: "search".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            thinking: None,
            metadata: None,
        }];

        let converted = CrossBoundaryConverter::convert(
            &messages,
            &BoundaryType::WorkflowToAgent,
            &BoundaryType::AgentToWorkflow,
        );
        assert!(converted[0].tool_calls.is_none());
    }

    #[test]
    fn test_inject_context() {
        let messages = vec![Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("Hello".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }];

        let mut vars = std::collections::HashMap::new();
        vars.insert("key1".to_string(), "value1".to_string());

        let result = CrossBoundaryConverter::inject_context(&messages, &vars);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, MessageRole::System);
    }
}
