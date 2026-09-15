use serde::{Deserialize, Serialize};

/// How the model list of a provider is discovered. Discovery never runs on
/// the request hot path; it serves model listing and assembly-time checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelDiscovery {
    /// `GET {base_url}/{api_version}/{path}`, ids extracted via `json_path`
    /// (defaults: `v1` / `models` / `data[*].id`).
    ModelsEndpoint {
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        json_path: Option<String>,
    },
    /// Fully custom URL, overriding the default base-url joining.
    CustomEndpoint {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        method: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<crate::Metadata>,
        #[serde(skip_serializing_if = "Option::is_none")]
        json_path: Option<String>,
    },
    /// Inline list loaded from configuration, no request sent.
    StaticList { models: Vec<super::ModelInfo> },
    /// No discovery; the profile model must be filled in by hand.
    Disabled,
}
