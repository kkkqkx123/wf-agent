use std::collections::HashMap;
use wf_types::llm::LlmProfile;

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
