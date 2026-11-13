use wf_integration::{
    apply_fold_results, build_fold_notice, select_fold_candidates, FoldBatchRequest, FoldCandidate,
    FoldClient, ServiceError,
};
use wf_types::message::Message;

/// Fold execution parameters, resolved from the node config by the
/// calling shell. The service address is part of the baked snapshot, so
/// the operation never consults a second config source.
#[derive(Debug, Clone)]
pub struct FoldParams {
    /// Service base URL baked at template assembly; `None` means the
    /// service was absent and the operation skips silently.
    pub base_url: Option<String>,
    /// Overall timeout for one service call in milliseconds.
    pub timeout_ms: u64,
    /// Wall-clock budget for the whole fold stage in milliseconds; the
    /// stage ends with a skip instead of holding the compression chain open.
    pub total_budget_ms: u64,
    /// Tool-result messages below this estimated token count are never sent.
    pub min_tokens: usize,
    /// Per-entry token budget requested from the fold service.
    pub max_tokens: usize,
    /// Maximum entries per batch request (local send intent).
    pub max_items: usize,
    /// Maximum retry rounds for failed batches.
    pub max_retries: u32,
}

impl Default for FoldParams {
    fn default() -> Self {
        let policy = wf_integration::FoldPolicy::default();
        Self {
            base_url: None,
            timeout_ms: 60_000,
            total_budget_ms: policy.total_budget_ms,
            min_tokens: policy.min_tokens,
            max_tokens: policy.max_tokens,
            max_items: policy.max_items,
            max_retries: policy.max_retries,
        }
    }
}

impl FoldParams {
    /// Clamp every numeric field to its executable minimum so a zero or
    /// undersized config can never produce an empty batch or a zero
    /// retry loop. Callers pass raw config; execution normalizes once.
    pub fn normalized(&self) -> Self {
        Self {
            base_url: self.base_url.clone(),
            timeout_ms: self.timeout_ms.max(1),
            total_budget_ms: self.total_budget_ms.max(1),
            min_tokens: self.min_tokens,
            max_tokens: self.max_tokens.max(1),
            max_items: self.max_items.max(1),
            max_retries: self.max_retries.max(1),
        }
    }

    /// True when no remote service is configured and only the local shell
    /// fold applies. Makes the skip path explicit at the call site.
    pub fn is_local_only(&self) -> bool {
        self.base_url
            .as_ref()
            .is_none_or(|url| url.trim().is_empty())
    }
}

/// Outcome of one fold operation over a message snapshot.
#[derive(Debug, Clone)]
pub struct FoldOutcome {
    /// Snapshot with folded texts written back plus the optional boundary
    /// notice headed when it saves budget.
    pub messages: Vec<Message>,
    /// Candidates successfully folded and written back.
    pub folded_count: usize,
    /// Estimated tokens before folding.
    pub original_tokens: usize,
    /// Estimated tokens after folding (notice included when headed).
    pub folded_tokens: usize,
    /// Whether the boundary notice was headed.
    pub notice_headed: bool,
    /// Skip or truncation reason; `None` on a clean run.
    pub skipped: Option<String>,
}

/// Run the fold operation over a message snapshot. Every service failure
/// maps to a skip with the context preserved; only the returned outcome
/// tells the shell what happened.
pub async fn execute_fold(messages: &[Message], params: &FoldParams) -> FoldOutcome {
    let params = params.normalized();
    let before = wf_llm::estimate_messages(messages) as usize;
    let finish = |messages: Vec<Message>,
                  folded_count: usize,
                  folded_tokens: usize,
                  notice_headed: bool,
                  skipped: Option<String>| FoldOutcome {
        messages,
        folded_count,
        original_tokens: before,
        folded_tokens,
        notice_headed,
        skipped,
    };
    let (locally_folded, local_count) = apply_local_fold(messages);
    let candidates = select_fold_candidates(&locally_folded, params.min_tokens, |message| {
        wf_llm::estimate_message_tokens(message) as usize
    });
    if candidates.is_empty() {
        if local_count > 0 {
            let folded_tokens = wf_llm::estimate_messages(&locally_folded) as usize;
            return finish(locally_folded, local_count, folded_tokens, false, None);
        }
        return finish(
            messages.to_vec(),
            0,
            before,
            false,
            Some("no foldable tool results".to_string()),
        );
    }
    let Some(base_url) = params.base_url.clone() else {
        if local_count > 0 {
            let folded_tokens = wf_llm::estimate_messages(&locally_folded) as usize;
            return finish(locally_folded, local_count, folded_tokens, false, None);
        }
        return finish(
            messages.to_vec(),
            0,
            before,
            false,
            Some("code-context service is not configured".to_string()),
        );
    };
    let client = match wf_integration::service_client() {
        Ok(http) => FoldClient::new(base_url, http),
        Err(reason) => {
            return finish(messages.to_vec(), 0, before, false, Some(reason));
        }
    };
    let max_tokens = params.max_tokens.max(1);
    let max_items = params.max_items.max(1);
    let max_retries = params.max_retries.max(1) as usize;
    let mut applied = locally_folded;
    let mut folded_count = local_count;
    let mut skipped: Option<String> = None;

    let mut pending: Vec<&[FoldCandidate]> = candidates.chunks(max_items).collect();
    let mut retry_round = 0usize;
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(params.total_budget_ms.max(1));

    'rounds: while !pending.is_empty() && retry_round <= max_retries {
        if std::time::Instant::now() >= deadline {
            skipped = Some("fold stage budget exhausted".to_string());
            break;
        }
        let mut next_pending: Vec<&[FoldCandidate]> = Vec::new();

        for chunk in &pending {
            let remaining_ms = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .max(1) as u64;
            let call_timeout_ms = params.timeout_ms.min(remaining_ms).max(1);
            match call_fold(&client, chunk, max_tokens, call_timeout_ms).await {
                Ok(results) => {
                    applied = apply_fold_results(&applied, chunk, &results);
                    folded_count += chunk.len();
                }
                Err(error) => {
                    if skipped.is_none() {
                        skipped = Some(error.reason.clone());
                    }
                    // An unavailable service rejects every further call, so
                    // splitting and retrying would only multiply refusals.
                    if error.is_unavailable() {
                        break 'rounds;
                    }
                    if chunk.len() > 1 {
                        let (left, right) = chunk.split_at(chunk.len() / 2);
                        next_pending.push(left);
                        next_pending.push(right);
                    } else {
                        next_pending.push(chunk);
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                skipped = Some("fold stage budget exhausted".to_string());
                break 'rounds;
            }
        }

        pending = next_pending;
        retry_round += 1;
    }

    if !pending.is_empty() && skipped.is_none() {
        skipped = Some(format!(
            "{} chunks failed after {} retries",
            pending.len(),
            retry_round
        ));
    }
    if folded_count == 0 && skipped.is_none() {
        skipped = Some("fold service returned no results".to_string());
    }
    let folded_without_notice = wf_llm::estimate_messages(&applied) as usize;
    let saved = before.saturating_sub(folded_without_notice);
    let notice = build_fold_notice(folded_count, saved);
    let notice_cost = wf_llm::estimate_message_tokens(&notice) as usize;
    let mut final_messages = applied;
    let mut notice_headed = false;
    if folded_count > 0 && folded_without_notice.saturating_add(notice_cost) < before {
        let mut headed = Vec::with_capacity(final_messages.len() + 1);
        headed.push(notice);
        headed.extend(final_messages);
        final_messages = headed;
        notice_headed = true;
    }
    let folded_tokens = wf_llm::estimate_messages(&final_messages) as usize;
    finish(
        final_messages,
        folded_count,
        folded_tokens,
        notice_headed,
        skipped,
    )
}

pub fn apply_local_fold(messages: &[Message]) -> (Vec<Message>, usize) {
    let mut folded = messages.to_vec();
    let mut count = 0usize;
    for message in &mut folded {
        if message.role != wf_types::message::MessageRole::Tool {
            continue;
        }
        let text = message.text_content();
        if text.trim().is_empty() {
            continue;
        }
        let Some(replacement) =
            wf_tools::predefined::fold::fold_tool_text(message.tool_name.as_deref(), &text)
        else {
            continue;
        };
        message.content = wf_types::message::MessageContentValue::Text(replacement);
        count += 1;
    }
    (folded, count)
}

async fn call_fold(
    client: &FoldClient,
    chunk: &[FoldCandidate],
    max_tokens: usize,
    timeout_ms: u64,
) -> Result<Vec<wf_integration::FoldBatchResultItem>, ServiceError> {
    let items: Vec<_> = chunk
        .iter()
        .cloned()
        .map(|candidate| candidate.into_item(max_tokens))
        .collect();
    let expected = items.len();
    let response = client
        .fold_batch(
            FoldBatchRequest {
                items,
                max_tokens: Some(max_tokens),
            },
            timeout_ms,
        )
        .await?;
    if response.results.len() != expected {
        return Err(ServiceError::protocol(format!(
            "fold result length mismatch: got {}, want {expected}",
            response.results.len()
        )));
    }
    Ok(response.results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_params_normalize_and_mark_local_only() {
        let raw = FoldParams {
            base_url: None,
            timeout_ms: 0,
            total_budget_ms: 0,
            min_tokens: 5,
            max_tokens: 0,
            max_items: 0,
            max_retries: 0,
        };
        let fixed = raw.normalized();
        assert_eq!(fixed.timeout_ms, 1);
        assert_eq!(fixed.total_budget_ms, 1);
        assert_eq!(fixed.max_tokens, 1);
        assert_eq!(fixed.max_items, 1);
        assert_eq!(fixed.max_retries, 1);
        assert!(fixed.is_local_only());
        let remote = FoldParams {
            base_url: Some("http://127.0.0.1:9".to_string()),
            ..FoldParams::default()
        };
        assert!(!remote.is_local_only());
    }

    #[tokio::test]
    async fn fold_skips_without_service_address() {
        let messages = vec![Message::user_text("hello".into())];
        let outcome = execute_fold(&messages, &FoldParams::default()).await;
        assert_eq!(outcome.folded_count, 0);
        assert!(outcome.skipped.is_some());
        assert_eq!(outcome.messages.len(), 1);
    }

    #[tokio::test]
    async fn fold_skips_unreachable_service() {
        let messages = vec![Message::tool_result(
            "call-1".into(),
            Some("read_file".into()),
            "fn main() {}\n".repeat(600),
            false,
        )];
        let outcome = execute_fold(
            &messages,
            &FoldParams {
                base_url: Some("http://127.0.0.1:1".into()),
                timeout_ms: 200,
                min_tokens: 1,
                ..FoldParams::default()
            },
        )
        .await;
        assert_eq!(outcome.folded_count, 0);
        assert!(outcome.skipped.is_some());
    }

    #[tokio::test]
    async fn fold_applies_local_shell_fold_without_service() {
        let text = (0..100)
            .map(|i| format!("output line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![Message::tool_result(
            "call-1".into(),
            Some("execute_command".into()),
            text,
            false,
        )];
        let outcome = execute_fold(&messages, &FoldParams::default()).await;
        assert_eq!(outcome.folded_count, 1);
        assert!(outcome.skipped.is_none());
        assert!(outcome.messages[0].text_content().contains("omitted"));
    }
}
