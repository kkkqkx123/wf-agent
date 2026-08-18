use std::collections::HashMap;
use wf_types::llm::LlmProfile;

/// Deep merge two JSON values:
/// - Arrays are concatenated
/// - Objects are recursively merged (source wins on key conflicts)
/// - Scalars are replaced by source
pub fn deep_merge(target: &serde_json::Value, source: &serde_json::Value) -> serde_json::Value {
    match (target, source) {
        (serde_json::Value::Object(t), serde_json::Value::Object(s)) => {
            let mut result = t.clone();
            for (k, v) in s {
                if let Some(existing) = result.get(k) {
                    result.insert(k.clone(), deep_merge(existing, v));
                } else {
                    result.insert(k.clone(), v.clone());
                }
            }
            serde_json::Value::Object(result)
        }
        (serde_json::Value::Array(t), serde_json::Value::Array(s)) => {
            let mut result = t.clone();
            result.extend(s.clone());
            serde_json::Value::Array(result)
        }
        _ => source.clone(),
    }
}

pub fn merge_parameters(
    profile: &LlmProfile,
    request_params: &Option<serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let mut merged = HashMap::new();

    if let Some(ref profile_params) = profile.parameters {
        if let Some(obj) = profile_params.as_object() {
            for (key, value) in obj {
                merged.insert(key.clone(), value.clone());
            }
        }
    }

    if let Some(ref params) = request_params {
        if let Some(obj) = params.as_object() {
            for (key, value) in obj {
                merged.insert(key.clone(), value.clone());
            }
        }
    }

    merged
}

/// Deep-merge profile parameters with request parameters.
pub fn deep_merge_parameters(
    profile: &LlmProfile,
    request_params: &Option<serde_json::Value>,
) -> serde_json::Value {
    let profile_obj = profile
        .parameters
        .clone()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    let request_obj = request_params
        .clone()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    deep_merge(&profile_obj, &request_obj)
}

pub fn build_auth_header(
    api_key: &Option<String>,
    native_header: &str,
) -> Option<(String, String)> {
    api_key
        .as_ref()
        .map(|key| (native_header.to_string(), key.clone()))
}

pub fn build_bearer_header(api_key: &Option<String>) -> Option<(String, String)> {
    api_key
        .as_ref()
        .map(|key| ("Authorization".to_string(), format!("Bearer {}", key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::{LlmProfile, LlmProvider};

    fn profile_with_params(params: Option<serde_json::Value>) -> LlmProfile {
        LlmProfile {
            id: "p1".to_string(),
            name: "test".to_string(),
            provider: LlmProvider::OpenaiChat,
            model: "gpt-4o".to_string(),
            api_key: None,
            base_url: None,
            parameters: params,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
        }
    }

    #[test]
    fn deep_merge_merges_objects_recursively() {
        let target = serde_json::json!({
            "a": 1,
            "nested": {"x": 1, "keep": "me"},
            "list": [1, 2],
        });
        let source = serde_json::json!({
            "b": 2,
            "nested": {"x": 99},
            "list": [3],
        });
        let merged = deep_merge(&target, &source);
        assert_eq!(merged["a"], serde_json::json!(1));
        assert_eq!(merged["b"], serde_json::json!(2));
        assert_eq!(merged["nested"]["x"], serde_json::json!(99), "source wins");
        assert_eq!(merged["nested"]["keep"], serde_json::json!("me"));
        assert_eq!(
            merged["list"],
            serde_json::json!([1, 2, 3]),
            "arrays concat"
        );
    }

    #[test]
    fn deep_merge_scalars_are_replaced_by_source() {
        assert_eq!(
            deep_merge(&serde_json::json!(1), &serde_json::json!(2)),
            serde_json::json!(2)
        );
        assert_eq!(
            deep_merge(&serde_json::json!({"a": 1}), &serde_json::json!("flat")),
            serde_json::json!("flat")
        );
        assert_eq!(
            deep_merge(&serde_json::json!([1]), &serde_json::json!({"o": 1})),
            serde_json::json!({"o": 1})
        );
    }

    #[test]
    fn merge_parameters_request_wins_on_conflicts() {
        let profile = profile_with_params(Some(serde_json::json!({
            "temperature": 0.7,
            "max_tokens": 100,
        })));
        let merged = merge_parameters(
            &profile,
            &Some(serde_json::json!({
                "temperature": 0.2,
                "extra": true,
            })),
        );
        assert_eq!(merged["temperature"], serde_json::json!(0.2));
        assert_eq!(merged["max_tokens"], serde_json::json!(100));
        assert_eq!(merged["extra"], serde_json::json!(true));
    }

    #[test]
    fn merge_parameters_handles_missing_and_non_object() {
        let profile = profile_with_params(None);
        assert!(merge_parameters(&profile, &None).is_empty());
        assert!(merge_parameters(&profile, &Some(serde_json::json!("nope"))).is_empty());

        let profile = profile_with_params(Some(serde_json::json!({"a": 1})));
        let merged = merge_parameters(&profile, &Some(serde_json::json!("nope")));
        assert_eq!(merged["a"], serde_json::json!(1));
    }

    #[test]
    fn deep_merge_parameters_combines_profile_and_request() {
        let profile = profile_with_params(Some(serde_json::json!({
            "temperature": 0.7,
            "nested": {"from_profile": true},
        })));
        let merged = deep_merge_parameters(
            &profile,
            &Some(serde_json::json!({
                "temperature": 0.1,
                "nested": {"from_request": true},
            })),
        );
        assert_eq!(merged["temperature"], serde_json::json!(0.1));
        assert_eq!(merged["nested"]["from_profile"], serde_json::json!(true));
        assert_eq!(merged["nested"]["from_request"], serde_json::json!(true));
    }

    #[test]
    fn auth_headers_are_built_correctly() {
        assert_eq!(
            build_auth_header(&Some("sk-1".to_string()), "x-api-key"),
            Some(("x-api-key".to_string(), "sk-1".to_string()))
        );
        assert_eq!(build_auth_header(&None, "x-api-key"), None);
        assert_eq!(
            build_bearer_header(&Some("sk-1".to_string())),
            Some(("Authorization".to_string(), "Bearer sk-1".to_string()))
        );
        assert_eq!(build_bearer_header(&None), None);
    }
}
