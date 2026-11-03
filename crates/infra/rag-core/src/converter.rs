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

/// HTML converter: strips tags, scripts, styles and comments, decodes
/// common entities and lifts the title element into metadata.
pub struct HtmlConverter;

#[async_trait]
impl DocumentConverter for HtmlConverter {
    fn name(&self) -> &'static str {
        "html"
    }

    fn extensions(&self) -> &[&str] {
        &["html", "htm"]
    }

    async fn convert(&self, _filename: &str, bytes: &[u8]) -> Result<ConvertedDocument> {
        let raw = String::from_utf8_lossy(bytes).to_string();
        let title = extract_title(&raw);
        let mut text = raw;
        for tag in ["script", "style"] {
            text = strip_block(&text, tag);
        }
        text = strip_comments(&text);
        let mut plain = String::with_capacity(text.len());
        let mut inside = false;
        for ch in text.chars() {
            match ch {
                '<' => {
                    inside = true;
                    plain.push('\n');
                }
                '>' => inside = false,
                _ if !inside => plain.push(ch),
                _ => {}
            }
        }
        let content = plain
            .lines()
            .map(|line| decode_entities(line.trim()))
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if content.is_empty() {
            return Err(RagError::Conversion("html input has no text".to_string()));
        }
        let mut metadata = HashMap::new();
        if let Some(title) = title {
            metadata.insert("title".to_string(), title);
        }
        Ok(ConvertedDocument { content, metadata })
    }
}

/// JSON converter preserving structure: emits one `path: value` line per
/// scalar, with object keys joined by dots and array items indexed.
/// Claims the same extension as [`JsonConverter`]; register it first to
/// prefer structure-preserving output for JSON sources.
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
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|err| RagError::Conversion(err.to_string()))?;
        let mut parts = Vec::new();
        collect_paths("", &value, &mut parts);
        let content = parts.join("\n").trim().to_string();
        if content.is_empty() {
            return Err(RagError::Conversion(
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
                collect_paths(&format!("{prefix}[{index}]",), item, parts);
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

fn strip_block(text: &str, tag: &str) -> String {
    let lower = text.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut lower_rest = lower.as_str();
    loop {
        let open = lower_rest.find(&format!("<{tag}"));
        let Some(start) = open else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let close_tag = format!("</{tag}>");
        match lower_rest[start..].find(&close_tag) {
            Some(end) => {
                let skip = start + end + close_tag.len();
                rest = &rest[skip..];
                lower_rest = &lower_rest[skip..];
            }
            None => break,
        }
    }
    out
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find("<!--") {
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start..].find("-->") {
                    Some(end) => rest = &rest[start + end + 3..],
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
}

fn extract_title(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = lower[start..].find("</title>")? + start;
    let title = decode_entities(text[start..end].trim());
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
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
        assert!(HtmlConverter.convert("a.html", b"<br>").await.is_err());
        assert!(PathJsonConverter.convert("a.json", b"{}").await.is_err());
    }

    #[tokio::test]
    async fn html_strips_tags_and_scripts() {
        let converted = HtmlConverter
            .convert(
                "a.html",
                b"<html><head><title>Doc</title><style>x{}</style></head><body><!-- hi --><script>bad()</script><h1>A &amp; B</h1><p>text</p></body></html>",
            )
            .await
            .expect("html");
        assert_eq!(
            converted.metadata.get("title").map(String::as_str),
            Some("Doc")
        );
        assert!(converted.content.contains("A & B"));
        assert!(converted.content.contains("text"));
        assert!(!converted.content.contains("bad()"));
        assert!(!converted.content.contains('<'));
    }

    #[tokio::test]
    async fn path_json_keeps_structure() {
        let converted = PathJsonConverter
            .convert(
                "a.json",
                br#"{"a": "first", "b": {"c": ["x", 7]}, "n": null}"#,
            )
            .await
            .expect("paths");
        assert!(converted.content.contains("a: first"));
        assert!(converted.content.contains("b.c[0]: x"));
        assert!(converted.content.contains("b.c[1]: 7"));
    }
}
