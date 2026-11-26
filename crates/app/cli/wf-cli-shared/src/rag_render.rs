//! Text presentation for retrieval hits shared by `wf rag` and tool output.

use std::io::Write;

use crate::error::CliResult;
use crate::output::OutputFormat;

const MAX_LINES: usize = 10;
const EXCERPT_CHARS: usize = 120;

fn text(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(value) = value.get(*key).and_then(serde_json::Value::as_str) {
            return value.to_string();
        }
    }
    String::new()
}

fn score(value: &serde_json::Value) -> Option<f64> {
    value.get("score").and_then(serde_json::Value::as_f64)
}

fn shorten(value: &str) -> String {
    if value.chars().count() <= EXCERPT_CHARS {
        return value.to_string();
    }
    let head: String = value.chars().take(EXCERPT_CHARS).collect();
    format!("{head}…")
}

/// Format a retrieval result as readable lines. Returns `None` when the
/// payload is not a hit array, so callers keep the generic JSON rendering.
pub fn format_hits(data: &serde_json::Value) -> Option<String> {
    let hits = data.as_array()?;
    if hits.is_empty() {
        return Some("no retrieval hits".to_string());
    }
    let lines = hits
        .iter()
        .take(MAX_LINES)
        .enumerate()
        .filter_map(|(index, hit)| {
            let chunk = text(hit, &["chunk_id", "chunkId"]);
            let source = text(hit, &["source", "document_id", "documentId"]);
            let excerpt = shorten(&text(hit, &["excerpt", "content"]));
            let score = score(hit)
                .map(|value| format!("{value:.3}"))
                .unwrap_or_default();
            if chunk.is_empty() && source.is_empty() && excerpt.is_empty() {
                return None;
            }
            Some(format!(
                "{}. [{}] {} {} — {}",
                index + 1,
                score,
                chunk,
                source,
                excerpt
            ))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Emit the human-readable hit list only in text mode; structured formats
/// keep their single-envelope contract.
pub fn render_hits(format: OutputFormat, data: &serde_json::Value) -> CliResult<()> {
    if format != OutputFormat::Text {
        return Ok(());
    }
    if let Some(text) = format_hits(data) {
        let mut stdout = std::io::stdout();
        writeln!(stdout, "{text}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_tool_hits_with_scores() {
        let data = serde_json::json!([{
            "chunk_id": "doc.md#0",
            "document_id": "doc.md",
            "source": "doc.md",
            "content": "ownership is recorded in the ledger",
            "score": 0.9123
        }]);
        let text = format_hits(&data).expect("hit list");
        assert!(text.contains("0.912"));
        assert!(text.contains("doc.md#0"));
        assert!(text.contains("ownership is recorded"));
    }

    #[test]
    fn reports_empty_results_and_ignores_other_payloads() {
        assert_eq!(
            format_hits(&serde_json::json!([])).as_deref(),
            Some("no retrieval hits")
        );
        assert!(format_hits(&serde_json::json!({"ok": true})).is_none());
    }
}
