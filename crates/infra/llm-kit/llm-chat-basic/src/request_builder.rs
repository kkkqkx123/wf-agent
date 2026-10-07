//! OpenAI-compatible JSON body builder shared by chat calls.
//!
//! Ported from code-context-engine `cce-llm-client` request builder: optional
//! fields are omitted from the body when unset so providers keep their
//! defaults.

use serde_json::{json, Value};

/// Fluent builder for an OpenAI-compatible request body.
#[derive(Debug, Clone, Default)]
pub struct RequestBuilder {
    body: Value,
}

impl RequestBuilder {
    /// Starts a body carrying the model name.
    pub fn new(model: &str) -> Self {
        Self {
            body: json!({ "model": model }),
        }
    }

    /// Adds the chat message list.
    pub fn with_messages(mut self, messages: Vec<Value>) -> Self {
        self.body["messages"] = json!(messages);
        self
    }

    /// Adds the max-tokens parameter.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.body["max_tokens"] = json!(max_tokens);
        self
    }

    /// Adds the temperature parameter.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.body["temperature"] = json!(temperature);
        self
    }

    /// Adds the top-p parameter.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.body["top_p"] = json!(top_p);
        self
    }

    /// Adds the frequency penalty when set.
    pub fn with_frequency_penalty(mut self, penalty: Option<f32>) -> Self {
        if let Some(value) = penalty {
            self.body["frequency_penalty"] = json!(value);
        }
        self
    }

    /// Adds the presence penalty when set.
    pub fn with_presence_penalty(mut self, penalty: Option<f32>) -> Self {
        if let Some(value) = penalty {
            self.body["presence_penalty"] = json!(value);
        }
        self
    }

    /// Adds stop sequences when non-empty.
    pub fn with_stop_sequences(mut self, sequences: &[String]) -> Self {
        if !sequences.is_empty() {
            self.body["stop"] = json!(sequences);
        }
        self
    }

    /// Adds the seed when set.
    pub fn with_seed(mut self, seed: Option<i64>) -> Self {
        if let Some(value) = seed {
            self.body["seed"] = json!(value);
        }
        self
    }

    /// Adds the response format wrapper (`{"type": ...}`).
    pub fn with_response_format(mut self, format_type: &str) -> Self {
        self.body["response_format"] = json!({ "type": format_type });
        self
    }

    /// Enables streaming responses (`"stream": true`).
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.body["stream"] = json!(stream);
        self
    }

    /// Returns the finished body.
    pub fn build(self) -> Value {
        self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_body_carries_core_parameters() {
        let messages = vec![
            json!({"role": "system", "content": "You are helpful"}),
            json!({"role": "user", "content": "Hello"}),
        ];
        let body = RequestBuilder::new("gpt-4")
            .with_messages(messages)
            .with_max_tokens(1000)
            .with_temperature(0.7)
            .with_top_p(1.0)
            .build();

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["max_tokens"], 1000);
        let temperature = body["temperature"].as_f64().expect("number");
        assert!((temperature - 0.7).abs() < 0.001);
        assert_eq!(body["messages"].as_array().expect("array").len(), 2);
    }

    #[test]
    fn optional_parameters_are_omitted_when_unset() {
        let body = RequestBuilder::new("gpt-4")
            .with_frequency_penalty(Some(0.5))
            .with_presence_penalty(None)
            .with_stop_sequences(&[])
            .with_seed(Some(42))
            .build();

        assert_eq!(body["frequency_penalty"], 0.5);
        let object = body.as_object().expect("object");
        assert!(!object.contains_key("presence_penalty"));
        assert!(!object.contains_key("stop"));
        assert_eq!(body["seed"], 42);
    }

    #[test]
    fn response_format_wraps_type_field() {
        let body = RequestBuilder::new("gpt-4")
            .with_response_format("json_object")
            .build();
        assert_eq!(body["response_format"]["type"], "json_object");
    }
}
