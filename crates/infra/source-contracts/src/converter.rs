//! Document conversion port and deterministic built-in converters.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Result, SourceError};

/// Normalized output of any converter.
#[derive(Debug, Clone)]
pub struct ConvertedDocument {
    /// Plain text for chunking.
    pub content: String,
    /// Converter-provided metadata (title, pages, detected format).
    pub metadata: HashMap<String, String>,
}

/// Port for document parsing, built-in or external.
#[async_trait]
pub trait DocumentConverter: Send + Sync {
    /// Converter name for logging and explicit selection.
    fn name(&self) -> &'static str;

    /// File extensions handled without the leading dot, lowercase.
    fn extensions(&self) -> &[&str];

    /// Parses bytes into normalized text.
    async fn convert(&self, filename: &str, bytes: &[u8]) -> Result<ConvertedDocument>;
}

/// Plain-text converter: UTF-8 lossy decode, no structure assumed.
pub struct PlainConverter;

#[async_trait]
impl DocumentConverter for PlainConverter {
    fn name(&self) -> &'static str {
        "plain"
    }

    fn extensions(&self) -> &[&str] {
        &["txt", "text", "log", "md", "markdown"]
    }

    async fn convert(&self, _filename: &str, bytes: &[u8]) -> Result<ConvertedDocument> {
        let content = String::from_utf8_lossy(bytes).trim().to_string();
        if content.is_empty() {
            return Err(SourceError::Conversion(
                "plain input has no text".to_string(),
            ));
        }
        Ok(ConvertedDocument {
            content,
            metadata: HashMap::new(),
        })
    }
}

/// Markdown converter: keeps the raw text and lifts the first heading as title.
pub struct MarkdownConverter;

#[async_trait]
impl DocumentConverter for MarkdownConverter {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn extensions(&self) -> &[&str] {
        &["md", "markdown"]
    }

    async fn convert(&self, _filename: &str, bytes: &[u8]) -> Result<ConvertedDocument> {
        let content = String::from_utf8_lossy(bytes).trim().to_string();
        if content.is_empty() {
            return Err(SourceError::Conversion(
                "markdown input has no text".to_string(),
            ));
        }
        let mut metadata = HashMap::new();
        for line in content.lines() {
            let line = line.trim().trim_start_matches('#').trim();
            if !line.is_empty() {
                metadata.insert("title".to_string(), line.to_string());
                break;
            }
        }
        Ok(ConvertedDocument { content, metadata })
    }
}

/// JSON converter: collects every string value in document order.
pub struct JsonConverter;

#[async_trait]
impl DocumentConverter for JsonConverter {
    fn name(&self) -> &'static str {
        "json"
    }

    fn extensions(&self) -> &[&str] {
        &["json"]
    }

    async fn convert(&self, _filename: &str, bytes: &[u8]) -> Result<ConvertedDocument> {
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|err| SourceError::Conversion(err.to_string()))?;
        let mut parts = Vec::new();
        collect_strings(&value, &mut parts);
        let content = parts.join("\n").trim().to_string();
        if content.is_empty() {
            return Err(SourceError::Conversion(
                "json input has no string values".to_string(),
            ));
        }
        Ok(ConvertedDocument {
            content,
            metadata: HashMap::new(),
        })
    }
}

fn collect_strings(value: &serde_json::Value, parts: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => {
            if !text.trim().is_empty() {
                parts.push(text.clone());
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_strings(item, parts);
            }
        }
        serde_json::Value::Object(map) => {
            for value in map.values() {
                collect_strings(value, parts);
            }
        }
        _ => {}
    }
}

/// JSON converter preserving structure: one `path: value` line per scalar.
pub struct PathJsonConverter;

#[async_trait]
impl DocumentConverter for PathJsonConverter {
    fn name(&self) -> &'static str {
        "json-paths"
    }

    fn extensions(&self) -> &[&str] {
        &["json"]
    }

    async fn convert(&self, _filename: &str, bytes: &[u8]) -> Result<ConvertedDocument> {
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|err| SourceError::Conversion(err.to_string()))?;
        let mut parts = Vec::new();
        collect_paths("", &value, &mut parts);
        let content = parts.join("\n").trim().to_string();
        if content.is_empty() {
            return Err(SourceError::Conversion(
                "json input has no scalar values".to_string(),
            ));
        }
        Ok(ConvertedDocument {
            content,
            metadata: HashMap::new(),
        })
    }
}

fn collect_paths(prefix: &str, value: &serde_json::Value, parts: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => {
            if !text.trim().is_empty() {
                parts.push(format!("{prefix}: {text}"));
            }
        }
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
            parts.push(format!("{prefix}: {value}"));
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_paths(&format!("{prefix}[{index}]"), item, parts);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_paths(&path, item, parts);
            }
        }
        serde_json::Value::Null => {}
    }
}

/// Returns the first converter claiming the filename extension.
pub fn converter_for<'a>(
    filename: &str,
    converters: &'a [&'a dyn DocumentConverter],
) -> Option<&'a dyn DocumentConverter> {
    let extension = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    converters
        .iter()
        .find(|converter| {
            converter
                .extensions()
                .iter()
                .any(|known| *known == extension)
        })
        .copied()
}
