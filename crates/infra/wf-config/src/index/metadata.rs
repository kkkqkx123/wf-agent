use super::*;

/// Extract metadata from an LLM Profile config (`id` / `name` /
/// `description` / `tags` / `format` / `provider_id` / `model`).
pub fn extract_llm_profile_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    let mut extra = serde_json::Map::new();
    if let Some(f) = optional_string(value, "format") {
        extra.insert("format".to_string(), serde_json::Value::String(f));
    }
    if let Some(p) = optional_string(value, "provider_id") {
        extra.insert("provider_id".to_string(), serde_json::Value::String(p));
    }
    if let Some(m) = optional_string(value, "model") {
        extra.insert("model".to_string(), serde_json::Value::String(m));
    }
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from an LLM Provider Definition config (`id` / `name` /
/// `description` / `tags` / `format` / `base_url`).
pub fn extract_llm_provider_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    let mut extra = serde_json::Map::new();
    if let Some(f) = optional_string(value, "format") {
        extra.insert("format".to_string(), serde_json::Value::String(f));
    }
    if let Some(b) = optional_string(value, "base_url") {
        extra.insert("base_url".to_string(), serde_json::Value::String(b));
    }
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Workflow config (`id` / `name` / `description` /
/// `tags` / `type` / `version` / `author`).
pub fn extract_workflow_metadata(value: &serde_json::Value, file_path: &str) -> ResolvedIndexEntry {
    let mut extra = serde_json::Map::new();
    if let Some(t) = optional_string(value, "type") {
        extra.insert("type".to_string(), serde_json::Value::String(t));
    }
    if let Some(v) = optional_string(value, "version") {
        extra.insert("version".to_string(), serde_json::Value::String(v));
    }
    if let Some(a) = value
        .get("metadata")
        .and_then(|m| m.get("author"))
        .and_then(|v| v.as_str())
    {
        extra.insert(
            "author".to_string(),
            serde_json::Value::String(a.to_string()),
        );
    }
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags")
            .or_else(|| value.get("metadata").and_then(|m| optional_tags(m, "tags"))),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Node Template config (id = `name`; `name` /
/// `description` / `node_type`).
pub fn extract_node_template_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    let name = optional_string(value, "name");
    let mut extra = serde_json::Map::new();
    if let Some(t) = optional_string(value, "node_type") {
        extra.insert("node_type".to_string(), serde_json::Value::String(t));
    }
    build_entry(
        name.clone().unwrap_or_default(),
        name,
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Script config (`id` = `id` or `name`; `name` /
/// `description` / `tags` / `category` / `executor`).
pub fn extract_script_metadata(value: &serde_json::Value, file_path: &str) -> ResolvedIndexEntry {
    let id = optional_string(value, "id")
        .or_else(|| optional_string(value, "name"))
        .unwrap_or_default();
    let mut extra = serde_json::Map::new();
    if let Some(e) =
        optional_string(value, "executor").or_else(|| optional_string(value, "executor_type"))
    {
        extra.insert("executor".to_string(), serde_json::Value::String(e));
    }
    build_entry(
        id,
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        optional_string(value, "category"),
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Prompt Template config (`id` / `name` /
/// `description` / `tags` / `category`).
pub fn extract_prompt_template_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        optional_string(value, "category"),
        file_path,
        serde_json::Value::Null,
    )
}

/// Extract metadata from an Agent Loop config (`id` / `name` /
/// `description` / `tags`).
pub fn extract_agent_loop_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags")
            .or_else(|| value.get("metadata").and_then(|m| optional_tags(m, "tags"))),
        None,
        file_path,
        serde_json::Value::Null,
    )
}
