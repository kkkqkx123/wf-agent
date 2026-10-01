use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use wf_types::message::{Message, MessageContent, MessageRole};

use crate::transport::{http_client, post_json};

/// One entry of a batch fold request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldBatchItem {
    /// Caller-provided stable identifier, echoed verbatim in the result.
    pub id: String,
    /// Raw source text to fold.
    pub text: String,
    /// Explicit language hint (highest priority).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// File name hint for suffix inference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// Caller token budget for the folded text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
}

impl FoldBatchItem {
    /// Create an item carrying an id and raw text.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            language: None,
            file_name: None,
            max_tokens: None,
        }
    }
}

/// Batch fold request with an optional global token-budget default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldBatchRequest {
    /// Entries to fold; result order matches this order.
    pub items: Vec<FoldBatchItem>,
    /// Global token budget default for items without an explicit budget.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
}

/// One entry of a batch fold response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldBatchResultItem {
    /// Echo of the caller-provided identifier.
    pub id: String,
    /// Folded skeleton text, or truncated source on degraded paths.
    pub folded_text: String,
    /// Language actually used for folding (`Unknown` on degraded paths).
    pub language: String,
    /// Whether the skeleton carries parsed structure.
    pub structure_known: bool,
    /// Token estimate before folding.
    pub original_tokens: usize,
    /// Token estimate after folding.
    pub folded_tokens: usize,
    /// Sections kept in the skeleton.
    pub kept_sections: usize,
    /// Sections dropped by the token budget.
    pub dropped_sections: usize,
}

/// Aggregate accounting over a batch fold response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoldBatchStats {
    /// Summed `original_tokens` over all entries.
    pub total_original_tokens: usize,
    /// Summed `folded_tokens` over all entries.
    pub total_folded_tokens: usize,
    /// Entries folded with known structure.
    pub structure_known_count: usize,
}

/// Batch fold response: per-entry results plus aggregate stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldBatchResponse {
    /// Per-entry results in request order.
    pub results: Vec<FoldBatchResultItem>,
    /// Aggregate token accounting for observation and budget checks.
    pub stats: FoldBatchStats,
}

/// Thin HTTP client for the stateless batch fold endpoint.
///
/// Constructed only when the service is configured; absence of a client is
/// the "folding disabled" gate. Any failure (unreachable service, timeout,
/// request-level rejection) surfaces as a string the caller maps to "skip
/// folding", never to a failure branch. Entry-level degradation arrives
/// in-band with `structure_known == false` and is applied like any result.
#[derive(Debug, Clone)]
pub struct FoldClient {
    client: reqwest::Client,
    base_url: String,
    timeout_ms: u64,
}

impl FoldClient {
    /// Build a client for the given service base URL.
    pub fn new(base_url: impl Into<String>, timeout_ms: u64) -> Result<Self, String> {
        Ok(Self {
            client: http_client(timeout_ms)?,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            timeout_ms: timeout_ms.max(1),
        })
    }

    /// Fold one batch with a single overall timeout. Any transport failure,
    /// timeout or request-level rejection is an error the caller maps to a
    /// silent skip; entry-level degradation stays in-band.
    pub async fn fold_batch(&self, request: FoldBatchRequest) -> Result<FoldBatchResponse, String> {
        if request.items.is_empty() {
            return Err("fold batch items must not be empty".to_string());
        }
        let url = format!("{}/api/tools/fold/batch", self.base_url);
        let body = serde_json::to_value(&request)
            .map_err(|e| format!("fold batch request was not encodable: {e}"))?;
        let payload = post_json(&self.client, &url, self.timeout_ms, &body).await?;
        serde_json::from_value::<FoldBatchResponse>(payload)
            .map_err(|e| format!("Fold batch response was not decodable: {e}"))
    }
}

/// A snapshot message selected for folding with its language clues.
#[derive(Debug, Clone)]
pub struct FoldCandidate {
    /// Index into the snapshot array for write-back.
    pub index: usize,
    /// Raw text sent to the service.
    pub text: String,
    /// Explicit language hint, if the file suffix mapped to one.
    pub language: Option<String>,
    /// File name hint for server-side suffix inference.
    pub file_name: Option<String>,
}

impl FoldCandidate {
    /// Stable batch identifier for this candidate within one snapshot.
    pub fn id(&self) -> String {
        format!("msg-{}", self.index)
    }

    /// Convert into a batch item carrying the caller's token budget.
    pub fn into_item(self, max_tokens: usize) -> FoldBatchItem {
        let mut item = FoldBatchItem::new(self.id(), self.text);
        item.language = self.language;
        item.file_name = self.file_name;
        item.max_tokens = Some(max_tokens);
        item
    }
}

/// Map `tool_call_id` to the `path` argument of the matching tool-use
/// block, so tool-result messages can recover their file clues.
pub fn tool_call_paths(messages: &[Message]) -> HashMap<String, String> {
    let mut paths = HashMap::new();
    for message in messages {
        let wf_types::message::MessageContentValue::Rich(blocks) = &message.content else {
            continue;
        };
        for block in blocks {
            let MessageContent::ToolUse { tool_use } = block else {
                continue;
            };
            if let Some(path) = tool_use.input.get("path").and_then(|v| v.as_str()) {
                paths.insert(tool_use.id.clone(), path.to_string());
            }
        }
    }
    paths
}

/// Select the snapshot entries worth folding: tool-result messages whose
/// estimated size reaches the threshold. Pure function; the caller supplies
/// token estimates from its own estimator. Non-tool messages are left
/// untouched so conversation structure survives.
pub fn select_fold_candidates(
    messages: &[Message],
    min_tokens: usize,
    estimate: impl Fn(&Message) -> usize,
) -> Vec<FoldCandidate> {
    let paths = tool_call_paths(messages);
    let mut candidates = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.role != MessageRole::Tool {
            continue;
        }
        if estimate(message) < min_tokens {
            continue;
        }
        let text = message.text_content();
        if text.trim().is_empty() {
            continue;
        }
        let path = message.tool_call_id.as_deref().and_then(|id| paths.get(id));
        let (language, file_name) = match path {
            Some(path) => (infer_language_hint(path), file_base_name(path)),
            None => (None, None),
        };
        candidates.push(FoldCandidate {
            index,
            text,
            language,
            file_name,
        });
    }
    candidates
}

/// Suggest a service language name from a file suffix. Only a pass-through
/// clue: unknown suffixes yield `None` and the service falls back to its
/// own file-name inference. No skeleton logic lives here.
pub fn infer_language_hint(path: &str) -> Option<String> {
    let extension = path.rsplit('.').next()?.trim().to_lowercase();
    if extension.is_empty() || extension == path.trim().to_lowercase() {
        return None;
    }
    let language = match extension.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "jsx" => "javascript",
        "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "tsx" => "typescript",
        "mts" | "cts" => "typescript",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "sh" | "bash" => "bash",
        "lua" => "lua",
        "r" => "r",
        "jl" => "julia",
        "hs" => "haskell",
        "ml" | "mli" => "ocaml",
        "ex" | "exs" => "elixir",
        "erl" | "hrl" => "erlang",
        "zig" => "zig",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "xml" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "md" | "markdown" => "markdown",
        "sql" => "sql",
        _ => return None,
    };
    Some(language.to_string())
}

/// Base file name of a path for the service file-name hint.
pub fn file_base_name(path: &str) -> Option<String> {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    (!base.is_empty()).then(|| base.to_string())
}

/// Build the boundary notice the model must see alongside folded products.
pub fn build_fold_notice(folded: usize, saved_tokens: usize) -> Message {
    Message::system_text(format!(
        "[context compression notice] {folded} file content(s) were folded into \
         symbol skeletons to save approximately {saved_tokens} tokens. Skeletons \
         keep signatures and structure but omit bodies; earlier full text is \
         unavailable."
    ))
}

/// Write folded texts back into a snapshot copy at the candidate indexes.
/// Results keyed by an unknown id are ignored; untouched entries keep
/// their original text.
pub fn apply_fold_results(
    messages: &[Message],
    candidates: &[FoldCandidate],
    results: &[FoldBatchResultItem],
) -> Vec<Message> {
    let by_id: HashMap<&str, &FoldBatchResultItem> =
        results.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut folded = messages.to_vec();
    for candidate in candidates {
        let Some(result) = by_id.get(candidate.id().as_str()) else {
            continue;
        };
        let Some(message) = folded.get_mut(candidate.index) else {
            continue;
        };
        message.content = wf_types::message::MessageContentValue::Text(result.folded_text.clone());
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, ToolUseContent};

    fn tool_result(call_id: &str, content: &str) -> Message {
        Message::tool_result(
            call_id.to_string(),
            Some("read_file".into()),
            content.into(),
            false,
        )
    }

    fn tool_use(id: &str, path: &str) -> Message {
        Message {
            id: "use-1".into(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Rich(vec![MessageContent::ToolUse {
                tool_use: ToolUseContent {
                    id: id.into(),
                    name: "read_file".into(),
                    input: serde_json::json!({ "path": path }),
                },
            }]),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn infer_language_hint_maps_common_suffixes() {
        assert_eq!(infer_language_hint("src/main.rs").as_deref(), Some("rust"));
        assert_eq!(
            infer_language_hint("a/b/hello.py").as_deref(),
            Some("python")
        );
        assert_eq!(infer_language_hint("notes.txt"), None);
        assert_eq!(infer_language_hint("Makefile"), None);
    }

    #[test]
    fn select_fold_candidates_recovers_paths_and_skips_small() {
        let messages = vec![
            tool_use("call-1", "src/main.rs"),
            tool_result("call-1", &"fn main() {}\n".repeat(500)),
            tool_result("call-9", "tiny"),
        ];
        let candidates = select_fold_candidates(&messages, 100, |m| m.text_content().len());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].index, 1);
        assert_eq!(candidates[0].language.as_deref(), Some("rust"));
        assert_eq!(candidates[0].file_name.as_deref(), Some("main.rs"));

        let small = select_fold_candidates(&messages, 100_000, |m| m.text_content().len());
        assert!(small.is_empty());
    }

    #[test]
    fn apply_fold_results_replaces_only_known_ids() {
        let messages = vec![
            Message::user_text("hello".into()),
            tool_result("call-1", "original"),
        ];
        let candidates = vec![FoldCandidate {
            index: 1,
            text: "original".into(),
            language: None,
            file_name: None,
        }];
        let results = vec![FoldBatchResultItem {
            id: "msg-1".into(),
            folded_text: "folded".into(),
            language: "Rust".into(),
            structure_known: true,
            original_tokens: 10,
            folded_tokens: 2,
            kept_sections: 1,
            dropped_sections: 0,
        }];
        let folded = apply_fold_results(&messages, &candidates, &results);
        assert_eq!(folded[0].text_content(), "hello");
        assert_eq!(folded[1].text_content(), "folded");
    }
}
