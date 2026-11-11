//! Minimal native RAG advisory demo: one contribution of each retrieval
//! extension kind, all deterministic and side-effect free. Host wiring is
//! described in README.md next to this crate.

use serde_json::{json, Value};
use wf_plugin_sdk::{
    export_plugin, NativeRegistrar, PluginError, PluginManifest, PluginResult, PluginType,
    WfNativePlugin,
};

pub struct RagAdvisoryDemo;

impl RagAdvisoryDemo {
    fn extractor_output(text: &str) -> Value {
        if text.to_lowercase().contains("rust") {
            json!({"topic": "rust"})
        } else {
            json!({})
        }
    }

    fn splitter_output(text: &str) -> Value {
        Value::Array(
            text.split("\n\n")
                .map(str::trim)
                .filter(|block| !block.is_empty())
                .map(|block| json!({"content": block}))
                .collect(),
        )
    }

    fn selector_output(query: &str, candidates: &[String]) -> Value {
        let lowered = query.to_lowercase();
        let kept: Vec<&str> = candidates
            .iter()
            .map(String::as_str)
            .filter(|name| lowered.contains(&name.to_lowercase()))
            .collect();
        json!({"targets": kept})
    }

    fn postprocessor_output(input: &Value) -> Value {
        let mut ids: Vec<String> = input
            .get("hits")
            .and_then(|hits| hits.as_array())
            .map(|hits| {
                hits.iter()
                    .filter_map(|hit| {
                        hit.get("chunk_id")
                            .and_then(|id| id.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        json!({"order": ids})
    }

    fn judge_output(answer: &str) -> Value {
        if answer.contains('[') && answer.contains(']') {
            json!({"score": 1.0, "reason": "answer carries citations"})
        } else {
            json!({"score": 0.5, "reason": "answer carries no citations"})
        }
    }

    fn text_arg(input: &Value) -> &str {
        input.get("text").and_then(Value::as_str).unwrap_or("")
    }
}

impl WfNativePlugin for RagAdvisoryDemo {
    fn new() -> Self {
        Self
    }

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "rag-advisory-demo".into(),
            version: "1.0.0".into(),
            name: None,
            description: None,
            plugin_type: Some(PluginType::Native),
            entry_point: "rag_advisory_demo.so".into(),
            versioned_entry_points: Default::default(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            tools: vec![],
            tool_descriptions: vec![],
            cli_commands: vec![],
            completion_sources: vec![],
            setting_fields: vec![],
            tool_summaries: vec![],
            markdown_rules: vec![],
            wasm: None,
            lua: None,
        }
    }

    fn dispatch(&self, kind: &str, name: &str, input: &Value) -> PluginResult<Value> {
        match (kind, name) {
            ("metadata-extractor", "demo-topic") => {
                Ok(Self::extractor_output(Self::text_arg(input)))
            }
            ("chunk-splitter", "demo-paragraph") => {
                Ok(Self::splitter_output(Self::text_arg(input)))
            }
            ("route-selector", "demo-keyword") => {
                let query = input.get("query").and_then(Value::as_str).unwrap_or("");
                let candidates: Vec<String> = input
                    .get("candidates")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|value| value.as_str())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Self::selector_output(query, &candidates))
            }
            ("postprocessor", "demo-ordered") => Ok(Self::postprocessor_output(input)),
            ("eval-judge", "demo-cited") => {
                let answer = input.get("answer").and_then(Value::as_str).unwrap_or("");
                Ok(Self::judge_output(answer))
            }
            _ => Err(PluginError::NativeError(format!(
                "unknown handler '{kind}/{name}'"
            ))),
        }
    }

    fn register(registrar: &NativeRegistrar) {
        registrar.register_metadata_extractor("demo-topic");
        registrar.register_chunk_splitter("demo-paragraph");
        registrar.register_route_selector("demo-keyword");
        registrar.register_postprocessor("demo-ordered");
        registrar.register_eval_judge("demo-cited");
    }
}

export_plugin!(RagAdvisoryDemo);

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> RagAdvisoryDemo {
        RagAdvisoryDemo
    }

    #[test]
    fn extractor_suggests_topic_only_on_keyword() {
        let out = plugin()
            .dispatch(
                "metadata-extractor",
                "demo-topic",
                &json!({"text": "Rust ownership", "options": {}}),
            )
            .expect("dispatch");
        assert_eq!(out, json!({"topic": "rust"}));
        let out = plugin()
            .dispatch(
                "metadata-extractor",
                "demo-topic",
                &json!({"text": "nothing relevant", "options": {}}),
            )
            .expect("dispatch");
        assert_eq!(out, json!({}));
    }

    #[test]
    fn splitter_cuts_on_blank_lines() {
        let out = plugin()
            .dispatch(
                "chunk-splitter",
                "demo-paragraph",
                &json!({"text": "first\n\nsecond", "options": {}}),
            )
            .expect("dispatch");
        assert_eq!(out, json!([{"content": "first"}, {"content": "second"}]));
    }

    #[test]
    fn unknown_handler_fails() {
        assert!(plugin()
            .dispatch("postprocessor", "nope", &json!({}))
            .is_err());
    }
}
