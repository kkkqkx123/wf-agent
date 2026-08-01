use std::collections::HashMap;
use wf_types::llm::LlmProfile;

/// Deep merge two JSON values following TS semantics:
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
