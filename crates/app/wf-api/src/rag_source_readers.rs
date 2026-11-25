//! Built-in source readers backed by the shared web transport and local files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use source_contracts::error::{Result, SourceError};
use source_contracts::{
    SourceDescriptor, SourceDocument, SourcePage, SourceReader, SourceRevision,
};
use wf_tools::predefined::web::{render_fetched_page, WebFetchBackend, WebFetchQuery};
use wf_tools::web_text::HtmlLinkMode;

/// Reads one web page through the host fetch backend.
pub struct UrlSourceReader {
    fetch: Arc<dyn WebFetchBackend>,
    max_content_chars: usize,
}

impl UrlSourceReader {
    pub fn new(fetch: Arc<dyn WebFetchBackend>, max_content_chars: usize) -> Self {
        Self {
            fetch,
            max_content_chars: max_content_chars.max(1),
        }
    }
}

#[async_trait]
impl SourceReader for UrlSourceReader {
    fn kind(&self) -> &'static str {
        "url"
    }

    async fn read(
        &self,
        descriptor: &SourceDescriptor,
        _cursor: Option<&str>,
    ) -> Result<SourcePage> {
        let mut query = WebFetchQuery::new(descriptor.uri.clone());
        query.link_mode = HtmlLinkMode::Inline;
        query.focus = descriptor.string_option("focus");
        query.selector = descriptor.string_option("selector");
        query.exclude_selector = descriptor.string_option("exclude_selector");
        query.min_words = descriptor
            .string_option("min_words")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let response = self
            .fetch
            .fetch(query.clone())
            .await
            .map_err(|err| SourceError::Read(err.to_string()))?;
        let rendered = render_fetched_page(&query, &response, "markdown", None)
            .map_err(|err| SourceError::Read(err.to_string()))?;
        let content = truncate_chars(
            &append_table_summaries(rendered.content, &rendered.tables),
            self.max_content_chars,
        );
        if content.trim().is_empty() {
            return Err(SourceError::Read(format!(
                "page '{}' has no searchable content",
                descriptor.uri
            )));
        }
        let fetched_at = Utc::now().to_rfc3339();
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(
            "title".to_string(),
            title_for(&rendered.title, &descriptor.uri),
        );
        metadata.insert("final_url".to_string(), response.final_url);
        metadata.insert("fetched_at".to_string(), fetched_at.clone());
        let revision = SourceRevision {
            content_hash: String::new(),
            etag: header_value(&response.headers, "etag"),
            last_modified: header_value(&response.headers, "last-modified"),
            fetched_at: Some(fetched_at),
        };
        Ok(SourcePage {
            documents: vec![
                SourceDocument::new(descriptor.source.clone(), content).with_metadata(metadata)
            ],
            revision,
            next_cursor: None,
        })
    }
}

/// Reads one file or a flat directory of files under configured roots.
pub struct FileSourceReader {
    roots: Vec<PathBuf>,
    max_bytes: usize,
}

impl FileSourceReader {
    pub fn new(roots: Vec<PathBuf>, max_bytes: usize) -> Self {
        Self {
            roots,
            max_bytes: max_bytes.max(1),
        }
    }

    fn resolve(&self, uri: &str) -> Result<PathBuf> {
        let path = PathBuf::from(uri);
        let resolved = path
            .canonicalize()
            .map_err(|err| SourceError::Read(format!("cannot open '{uri}': {err}")))?;
        if !self.roots.is_empty() && !self.roots.iter().any(|root| resolved.starts_with(root)) {
            return Err(SourceError::Read(format!(
                "path '{uri}' is outside configured source roots"
            )));
        }
        Ok(resolved)
    }

    fn files(path: &Path) -> Result<Vec<PathBuf>> {
        if path.is_file() {
            return Ok(vec![path.to_path_buf()]);
        }
        if !path.is_dir() {
            return Err(SourceError::Read(format!(
                "source path '{}' is neither file nor directory",
                path.display()
            )));
        }
        let mut files = std::fs::read_dir(path)
            .map_err(|err| SourceError::Read(format!("cannot list '{}': {err}", path.display())))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .collect::<Vec<_>>();
        files.sort();
        Ok(files)
    }
}

#[async_trait]
impl SourceReader for FileSourceReader {
    fn kind(&self) -> &'static str {
        "file"
    }

    async fn read(
        &self,
        descriptor: &SourceDescriptor,
        _cursor: Option<&str>,
    ) -> Result<SourcePage> {
        let resolved = self.resolve(&descriptor.uri)?;
        let files = Self::files(&resolved)?;
        let mut documents = Vec::with_capacity(files.len());
        for file in files {
            let bytes = std::fs::read(&file).map_err(|err| {
                SourceError::Read(format!("cannot read '{}': {err}", file.display()))
            })?;
            if bytes.len() > self.max_bytes {
                return Err(SourceError::Read(format!(
                    "source file '{}' exceeds {} bytes",
                    file.display(),
                    self.max_bytes
                )));
            }
            let content = String::from_utf8_lossy(&bytes).trim().to_string();
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "filename".to_string(),
                file.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            );
            let id = if documents.is_empty() {
                descriptor.source.clone()
            } else {
                format!("{}#{}", descriptor.source, documents.len())
            };
            documents.push(SourceDocument::new(id, content).with_metadata(metadata));
        }
        Ok(SourcePage {
            documents,
            revision: SourceRevision::default(),
            next_cursor: None,
        })
    }
}

fn title_for(rendered: &str, fallback: &str) -> String {
    let trimmed = rendered.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn header_value(headers: &BTreeMap<String, String>, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

/// Truncates by characters so multibyte sources stay intact.
pub fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

/// Appends table rows to the rendered page so tabular facts remain searchable.
pub fn append_table_summaries(content: String, tables: &serde_json::Value) -> String {
    let Some(rows) = tables.as_array() else {
        return content;
    };
    if rows.is_empty() {
        return content;
    }
    let mut out = content;
    out.push_str("\n\nIndexed tables:\n");
    for table in rows {
        let headers = table
            .get("headers")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.as_str().unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !headers.is_empty() {
            out.push_str(&format!("| {} |\n", headers.join(" | ")));
            out.push_str(&format!("|{}|\n", " --- |".repeat(headers.len())));
        }
        if let Some(rows) = table.get("rows").and_then(serde_json::Value::as_array) {
            for row in rows.iter().take(50) {
                let cells = row
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|value| value.as_str().unwrap_or_default().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if !cells.is_empty() {
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                }
            }
        }
    }
    out
}
