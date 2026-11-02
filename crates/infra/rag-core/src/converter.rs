//! Document conversion: built-in light formats plus the external port.
//!
//! Built-in converters stay dependency-free and deterministic. Heavy
//! formats (PDF layout, formulas, tables, scanned pages) go through the
//! [`DocumentConverter`] port, implemented by external services such as
//! minerU over HTTP, MCP or plugins. Ingest normalizes every converter
//! output the same way, so the source of parsing never leaks downstream.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{RagError, Result};

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
    /// Converter name for logging (for example `plain`, `markdown`, `mineru`).
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
        &["txt", "text", "log"]
    }

    async fn convert(&self, _filename: &str, bytes: &[u8]) -> Result<ConvertedDocument> {
        let content = String::from_utf8_lossy(bytes).trim().to_string();
        if content.is_empty() {
            return Err(RagError::Conversion("plain input has no text".to_string()));
        }
        Ok(ConvertedDocument {
            content,
            metadata: HashMap::new(),
        })
    }
}

/// Markdown converter: keeps the raw text, lifts the first heading as title.
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
            return Err(RagError::Conversion(
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
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|err| RagError::Conversion(err.to_string()))?;
        let mut parts = Vec::new();
        collect_strings(&value, &mut parts);
        let content = parts.join("\n").trim().to_string();
        if content.is_empty() {
            return Err(RagError::Conversion(
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

/// Routes a file to the first converter claiming its extension.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plain_and_markdown_convert() {
        let plain = PlainConverter
            .convert("a.txt", b"hello")
            .await
            .expect("plain");
        assert_eq!(plain.content, "hello");

        let markdown = MarkdownConverter
            .convert("a.md", b"# Title\n\nbody")
            .await
            .expect("markdown");
        assert_eq!(
            markdown.metadata.get("title").map(String::as_str),
            Some("Title")
        );
        assert!(markdown.content.contains("body"));
    }

    #[tokio::test]
    async fn json_collects_strings() {
        let converted = JsonConverter
            .convert("a.json", br#"{"a": "first", "b": {"c": "second"}, "n": 1}"#)
            .await
            .expect("json");
        assert!(converted.content.contains("first"));
        assert!(converted.content.contains("second"));
    }

    #[tokio::test]
    async fn empty_inputs_fail() {
        assert!(PlainConverter.convert("a.txt", b"   ").await.is_err());
        assert!(JsonConverter.convert("a.json", b"{}").await.is_err());
    }
}
