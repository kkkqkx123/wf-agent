use wf_types::llm::{LlmProviderDefinition, ModelDiscovery, ModelInfo};

use crate::error::{LlmError, LlmResult};

/// Default path of the models endpoint below the version segment.
pub const DEFAULT_MODELS_PATH: &str = "models";
/// Default extraction rule covering OpenAI-style `{ "data": [{ "id": .. }] }`.
pub const DEFAULT_MODELS_JSON_PATH: &str = "data[*].id";

/// Off-hot-path model listing for a provider definition.
///
/// Serves CLI listing, API listing and assembly-time checks. Discovery
/// failures are reported as errors to the caller; callers at assembly time
/// degrade them to warnings and never block profile registration.
#[derive(Clone)]
pub struct ModelCatalog {
    client: reqwest::Client,
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// List the models advertized by a provider definition.
    ///
    /// `api_key` carries the effective profile key when the caller has one;
    /// definitions without remote discovery return without using it.
    pub async fn list_models(
        &self,
        definition: &LlmProviderDefinition,
        api_key: Option<&str>,
    ) -> LlmResult<Vec<ModelInfo>> {
        let discovery = definition
            .model_discovery
            .clone()
            .unwrap_or(ModelDiscovery::Disabled);
        match discovery {
            ModelDiscovery::Disabled => Ok(Vec::new()),
            ModelDiscovery::StaticList { models } => Ok(models),
            ModelDiscovery::ModelsEndpoint { path, json_path } => {
                let base_url = definition.base_url.clone().ok_or_else(|| {
                    LlmError::ConfigError(format!(
                        "Provider '{}' has no base_url for model discovery",
                        definition.id
                    ))
                })?;
                let url = join_models_url(
                    &base_url,
                    definition.api_version.as_deref(),
                    path.as_deref(),
                );
                let mut request = self.client.get(&url);
                request = apply_discovery_auth(
                    request,
                    definition.auth_type.as_deref().unwrap_or("native"),
                    api_key,
                );
                if let Some(headers) = definition.default_headers.as_ref() {
                    for (key, value) in headers {
                        request = request.header(key, value.as_str().unwrap_or(""));
                    }
                }
                let response = request.send().await?;
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Err(LlmError::ProviderError(format!(
                        "HTTP {}: {}",
                        status.as_u16(),
                        body
                    )));
                }
                let json: serde_json::Value = serde_json::from_str(&body)?;
                Ok(extract_models(
                    &json,
                    json_path.as_deref().unwrap_or(DEFAULT_MODELS_JSON_PATH),
                ))
            }
            ModelDiscovery::CustomEndpoint {
                url,
                method,
                headers,
                json_path,
            } => {
                let method = method.as_deref().unwrap_or("GET");
                let mut request = self.client.request(
                    method.parse().map_err(|_| {
                        LlmError::ConfigError(format!(
                            "Provider '{}' has an invalid discovery method '{}'",
                            definition.id, method
                        ))
                    })?,
                    &url,
                );
                if let Some(headers) = headers.as_ref() {
                    for (key, value) in headers {
                        request = request.header(key, value.as_str().unwrap_or(""));
                    }
                }
                let response = request.send().await?;
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Err(LlmError::ProviderError(format!(
                        "HTTP {}: {}",
                        status.as_u16(),
                        body
                    )));
                }
                let json: serde_json::Value = serde_json::from_str(&body)?;
                Ok(extract_models(
                    &json,
                    json_path.as_deref().unwrap_or(DEFAULT_MODELS_JSON_PATH),
                ))
            }
        }
    }
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// Join `{base_url}/{api_version}/{path}` tolerating trailing slashes and a
/// base URL that already ends with the version segment.
pub fn join_models_url(base_url: &str, api_version: Option<&str>, path: Option<&str>) -> String {
    let mut url = base_url.trim_end_matches('/').to_string();
    let version = api_version.unwrap_or("v1");
    if !version.is_empty() && !url.ends_with(&format!("/{version}")) {
        url.push('/');
        url.push_str(version);
    }
    let path = path.unwrap_or(DEFAULT_MODELS_PATH);
    if !path.is_empty() {
        url.push('/');
        url.push_str(path.trim_start_matches('/'));
    }
    url
}

fn apply_discovery_auth(
    request: reqwest::RequestBuilder,
    auth_type: &str,
    api_key: Option<&str>,
) -> reqwest::RequestBuilder {
    let Some(api_key) = api_key else {
        return request;
    };
    match auth_type {
        "x-api-key" => request.header("x-api-key", api_key),
        "x-goog-api-key" => request.header("x-goog-api-key", api_key),
        _ => request.header("Authorization", format!("Bearer {api_key}")),
    }
}

/// Extract model ids from a JSON document following a `a.b[*].c` rule.
///
/// A `[*]` segment maps over arrays; a trailing scalar segment yields ids
/// directly, while objects yield their `id` field when present.
pub fn extract_models(value: &serde_json::Value, json_path: &str) -> Vec<ModelInfo> {
    let segments: Vec<&str> = json_path.split('.').collect();
    let mut current = vec![value];
    for segment in segments {
        let mut next = Vec::new();
        if let Some(key) = segment.strip_suffix("[*]") {
            for item in current {
                let target = if key.is_empty() { item } else { &item[key] };
                if let Some(array) = target.as_array() {
                    next.extend(array.iter());
                }
            }
        } else {
            for item in current {
                let target = &item[segment];
                if !target.is_null() {
                    next.push(target);
                }
            }
        }
        current = next;
    }
    current
        .into_iter()
        .filter_map(|item| {
            if let Some(id) = item.as_str() {
                Some(ModelInfo {
                    id: id.to_string(),
                    name: None,
                    context_window_size: None,
                    metadata: None,
                })
            } else {
                item.get("id")
                    .and_then(|id| id.as_str())
                    .map(|id| ModelInfo {
                        id: id.to_string(),
                        name: item
                            .get("name")
                            .and_then(|name| name.as_str())
                            .map(String::from),
                        context_window_size: None,
                        metadata: None,
                    })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_endpoint_without_duplicating_version() {
        assert_eq!(
            join_models_url("https://api.test.com", None, None),
            "https://api.test.com/v1/models"
        );
        assert_eq!(
            join_models_url("https://api.test.com/", Some("v1"), Some("models")),
            "https://api.test.com/v1/models"
        );
        assert_eq!(
            join_models_url("https://api.test.com/v1", None, None),
            "https://api.test.com/v1/models"
        );
        assert_eq!(
            join_models_url("https://api.test.com", Some("v1beta"), None),
            "https://api.test.com/v1beta/models"
        );
    }

    #[test]
    fn extracts_openai_style_ids() {
        let body: serde_json::Value = serde_json::json!({
            "data": [{"id": "a"}, {"id": "b"}]
        });
        let models = extract_models(&body, "data[*].id");
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn extracts_object_ids_with_names() {
        let body: serde_json::Value = serde_json::json!({
            "models": [{"id": "m1", "name": "First"}]
        });
        let models = extract_models(&body, "models[*]");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "m1");
        assert_eq!(models[0].name.as_deref(), Some("First"));
    }

    #[test]
    fn unknown_paths_yield_empty() {
        let body: serde_json::Value = serde_json::json!({"data": []});
        assert!(extract_models(&body, "missing[*].id").is_empty());
    }
}
