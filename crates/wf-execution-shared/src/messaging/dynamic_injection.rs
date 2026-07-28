use wf_types::message::{Message, MessageContentValue, MessageRole};
use std::collections::HashMap;

pub struct DynamicInjection;

impl DynamicInjection {
    pub fn inject_variables(message: &Message, variables: &HashMap<String, String>) -> Message {
        let mut injected = message.clone();

        if let MessageContentValue::Text(text) = &injected.content {
            let mut result = text.clone();
            for (key, value) in variables {
                let placeholder = format!("{{{{{}}}}}", key);
                result = result.replace(&placeholder, value);
            }
            injected.content = MessageContentValue::Text(result);
        }

        injected
    }

    pub fn inject_context_messages(messages: &[Message], context: &HashMap<String, String>) -> Vec<Message> {
        if context.is_empty() {
            return messages.to_vec();
        }

        let mut result = Vec::new();

        let mut context_text = String::from("Dynamic context:\n");
        for (key, value) in context {
            context_text.push_str(&format!("  {}: {}\n", key, value));
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

        result.push(context_msg);
        result.extend_from_slice(messages);
        result
    }

    pub fn resolve_expressions(text: &str, variables: &HashMap<String, String>) -> String {
        let mut result = text.to_string();

        for (key, value) in variables {
            let placeholder = format!("${{{}}}", key);
            result = result.replace(&placeholder, value);

            let placeholder2 = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder2, value);
        }

        result
    }

    pub fn has_unresolved_variables(text: &str) -> bool {
        text.contains("${{") && text.contains("}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_variables() {
        let msg = Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text("Hello {{name}}".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let injected = DynamicInjection::inject_variables(&msg, &vars);
        match &injected.content {
            MessageContentValue::Text(text) => assert_eq!(text, "Hello World"),
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_resolve_expressions() {
        let mut vars = HashMap::new();
        vars.insert("user".to_string(), "Alice".to_string());

        let result = DynamicInjection::resolve_expressions("Hello ${user}!", &vars);
        assert_eq!(result, "Hello Alice!");
    }

    #[test]
    fn test_has_unresolved_variables() {
        assert!(DynamicInjection::has_unresolved_variables("Hello ${{name}}"));
        assert!(!DynamicInjection::has_unresolved_variables("Hello World"));
    }
}
