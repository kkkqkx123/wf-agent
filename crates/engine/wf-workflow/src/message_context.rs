//! Named message contexts shared by LLM / AGENT_LOOP / TOOL_VISIBILITY nodes.
//!
//! Contexts are stored in the workflow variable map under a reserved prefix
//! (`__msg_ctx__`) so they are part of the execution state (checkpoints and
//! variable snapshots) and readable from any node in the workflow.
//!
//! A per-array estimation ledger (`__msg_ledger__`, decision track) lives
//! next to each context: appends estimate only the new messages (O(new)),
//! replacements mark the entry dirty so the next read recomputes exactly
//! once. Budget checks read the ledger instead of rescanning the array.

use dashmap::DashMap;
use serde_json::Value;
use wf_types::llm::TokenLedger;
use wf_types::message::Message;

/// Variable-map prefix for named message contexts.
pub const CONTEXT_PREFIX: &str = "__msg_ctx__";
/// Variable-map prefix for the archived (append-only) history of a named
/// message context. Compression and re-registration relocate the previous
/// active messages here instead of dropping them, so context history is
/// never lost; the checkpoint promotes this into the `message_contexts`
/// domain.
pub const CONTEXT_HISTORY_PREFIX: &str = "__msg_ctx_hist__";
/// Variable-map prefix for the per-array estimation ledger (decision track).
pub const LEDGER_PREFIX: &str = "__msg_ledger__";
/// Default context used when a node does not name one.
pub const DEFAULT_CONTEXT_ID: &str = "current";

fn normalize_id(context_id: &str) -> String {
    if context_id.is_empty() {
        DEFAULT_CONTEXT_ID.to_string()
    } else {
        context_id.to_string()
    }
}

fn context_key(context_id: &str) -> String {
    format!("{}{}", CONTEXT_PREFIX, normalize_id(context_id))
}

fn history_key(context_id: &str) -> String {
    format!("{}{}", CONTEXT_HISTORY_PREFIX, normalize_id(context_id))
}

fn ledger(variables: &DashMap<String, Value>) -> TokenLedger {
    variables
        .get(LEDGER_PREFIX)
        .and_then(
            |entry| match serde_json::from_value::<TokenLedger>(entry.clone()) {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    tracing::warn!(
                        key = LEDGER_PREFIX,
                        error = %e,
                        "token ledger is corrupted, degrading to empty ledger"
                    );
                    None
                }
            },
        )
        .unwrap_or_default()
}

fn store_ledger(variables: &DashMap<String, Value>, ledger: &TokenLedger) {
    if let Ok(value) = serde_json::to_value(ledger) {
        variables.insert(LEDGER_PREFIX.to_string(), value);
    }
}

/// Read the message list of a named context (empty when absent). When the
/// ledger entry is dirty (array was replaced/restored), the estimate is
/// recomputed once and written back.
pub fn get_context(variables: &DashMap<String, Value>, context_id: &str) -> Vec<Message> {
    let messages: Vec<Message> = match variables.get(&context_key(context_id)) {
        Some(entry) => match serde_json::from_value(entry.clone()) {
            Ok(parsed) => parsed,
            Err(e) => {
                tracing::warn!(
                    key = context_key(context_id),
                    error = %e,
                    "message context is corrupted, degrading to empty context"
                );
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    let mut ledger = ledger(variables);
    if ledger.is_dirty(&normalize_id(context_id)) {
        let estimated = wf_llm::estimate_messages(&messages) as u64;
        ledger.recompute(&normalize_id(context_id), estimated, messages.len());
        store_ledger(variables, &ledger);
    }
    messages
}

/// Append messages to a named context, creating it when needed. The ledger
/// records only the newly added messages (incremental, O(new messages)).
pub fn append_context(
    variables: &DashMap<String, Value>,
    context_id: &str,
    messages: Vec<Message>,
) {
    if messages.is_empty() {
        return;
    }
    let added_tokens = wf_llm::estimate_messages(&messages) as u64;
    let added_count = messages.len();
    let mut existing = get_context(variables, context_id);
    existing.extend(messages);
    if let Ok(value) = serde_json::to_value(&existing) {
        variables.insert(context_key(context_id), value);
    }
    let mut ledger = ledger(variables);
    ledger.append(&normalize_id(context_id), added_tokens, added_count);
    store_ledger(variables, &ledger);
}

/// Replace the active content of a named context. History is preserved: the
/// previous active messages are moved into the context's archived history
/// (`CONTEXT_HISTORY_PREFIX`, message-id-deduplicated, append-only) before the
/// new content becomes the active view. The ledger entry is marked dirty
/// (estimate recomputed lazily on the next read) and its version bumped so
/// stale compression guards are invalidated.
pub fn register_context(
    variables: &DashMap<String, Value>,
    context_id: &str,
    messages: Vec<Message>,
) {
    // Archive the outgoing active messages instead of dropping them.
    let previous = get_context(variables, context_id);
    if !previous.is_empty() {
        let mut history = archived_history(variables, context_id);
        let known: std::collections::HashSet<String> =
            history.iter().map(|m| m.id.clone()).collect();
        for message in previous {
            if !known.contains(&message.id) {
                history.push(message);
            }
        }
        if let Ok(value) = serde_json::to_value(&history) {
            variables.insert(history_key(context_id), value);
        }
    }
    if let Ok(value) = serde_json::to_value(&messages) {
        variables.insert(context_key(context_id), value);
    }
    let mut ledger = ledger(variables);
    ledger.replace(&normalize_id(context_id));
    store_ledger(variables, &ledger);
}

/// Read the full lossless history of a named context: the archived
/// messages followed by the active view, deduplicated by message id.
/// This is the record checkpoints persist; `get_context` returns the
/// active view only (what LLM nodes assemble).
pub fn get_context_history(variables: &DashMap<String, Value>, context_id: &str) -> Vec<Message> {
    let mut full = archived_history(variables, context_id);
    let known: std::collections::HashSet<String> =
        full.iter().map(|m| m.id.clone()).collect();
    for message in get_context(variables, context_id) {
        if !known.contains(&message.id) {
            full.push(message);
        }
    }
    full
}

/// Read only the archived (already superseded) messages of a named
/// context, without the active view. Checkpoint builders merge both
/// sides via [`get_context_history`]; this accessor exists for
/// diagnostics and retention accounting.
pub fn archived_history(variables: &DashMap<String, Value>, context_id: &str) -> Vec<Message> {
    match variables.get(&history_key(context_id)) {
        Some(entry) => serde_json::from_value(entry.clone()).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Undo the last view switch (compression, filter, re-registration):
/// the full lossless history becomes the active view again and the
/// archive is cleared (its content now lives in the active array).
/// History was never deleted, so undoing costs nothing.
pub fn restore_full_history(variables: &DashMap<String, Value>, context_id: &str) {
    let full = get_context_history(variables, context_id);
    if let Ok(value) = serde_json::to_value(&full) {
        variables.insert(context_key(context_id), value);
    }
    variables.remove(&history_key(context_id));
    let mut ledger = ledger(variables);
    ledger.replace(&normalize_id(context_id));
    store_ledger(variables, &ledger);
}

/// Whether a named context has been registered (even when empty).
pub fn has_context(variables: &DashMap<String, Value>, context_id: &str) -> bool {
    variables.contains_key(&context_key(context_id))
}

/// Decision track: current ledger estimate of a named array (0 when the
/// array is not tracked; the caller must have read the array via
/// [`get_context`] first so a dirty entry was recomputed).
pub fn ledger_estimated_tokens(variables: &DashMap<String, Value>, context_id: &str) -> u64 {
    ledger(variables).estimated_tokens(&normalize_id(context_id))
}

/// Decision track: current version of a named array (0 when not tracked).
pub fn array_version(variables: &DashMap<String, Value>, context_id: &str) -> u64 {
    ledger(variables).version(&normalize_id(context_id))
}

/// Decision track: message count recorded for a named array.
pub fn ledger_message_count(variables: &DashMap<String, Value>, context_id: &str) -> usize {
    ledger(variables).message_count(&normalize_id(context_id))
}

/// Single-shot compression guard: may a request be emitted for the array at
/// `version`? False when the same version was already announced.
pub fn should_emit_compression(
    variables: &DashMap<String, Value>,
    context_id: &str,
    version: u64,
) -> bool {
    ledger(variables).should_emit(&normalize_id(context_id), version)
}

/// Record that a compression request was emitted for the array at `version`.
pub fn mark_compression_emitted(
    variables: &DashMap<String, Value>,
    context_id: &str,
    version: u64,
) {
    let mut ledger = ledger(variables);
    ledger.mark_emitted(&normalize_id(context_id), version);
    store_ledger(variables, &ledger);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use wf_types::message::{MessageContentValue, MessageRole};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    fn msg(role: MessageRole, text: &str) -> Message {
        Message {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed).to_string(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[test]
    fn context_roundtrip() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "hi")]);
        assert!(has_context(&vars, "chat"));
        let ctx = get_context(&vars, "chat");
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].role, MessageRole::User);

        register_context(&vars, "chat", vec![msg(MessageRole::Assistant, "yo")]);
        // The active view is the newly registered content...
        assert_eq!(get_context(&vars, "chat").len(), 1);
        assert_eq!(get_context(&vars, "chat")[0].role, MessageRole::Assistant);
        // ...the archive holds the superseded message...
        let archived = archived_history(&vars, "chat");
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].role, MessageRole::User);
        // ...and the full history merges both sides.
        let history = get_context_history(&vars, "chat");
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn register_context_archives_history_append_only() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "first")]);
        register_context(&vars, "chat", vec![msg(MessageRole::Assistant, "summary")]);
        append_context(&vars, "chat", vec![msg(MessageRole::User, "second")]);
        register_context(&vars, "chat", vec![msg(MessageRole::Assistant, "summary-2")]);

        // Active view holds only the latest registration.
        assert_eq!(get_context(&vars, "chat").len(), 1);
        // Archive holds every earlier active message, deduplicated by id.
        let archived = archived_history(&vars, "chat");
        assert_eq!(archived.len(), 3, "first + summary + second archived");
        // Full history merges the archive with the active view.
        let history = get_context_history(&vars, "chat");
        assert_eq!(history.len(), 4);
        let texts: Vec<&str> = history
            .iter()
            .filter_map(|m| match &m.content {
                MessageContentValue::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"first"));
        assert!(texts.contains(&"summary"));
        assert!(texts.contains(&"second"));
        assert!(texts.contains(&"summary-2"));
    }

    #[test]
    fn restore_full_history_undoes_view_switch_at_zero_cost() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "first")]);
        append_context(&vars, "chat", vec![msg(MessageRole::User, "second")]);
        register_context(&vars, "chat", vec![msg(MessageRole::Assistant, "summary")]);
        assert_eq!(get_context(&vars, "chat").len(), 1);

        restore_full_history(&vars, "chat");

        let view = get_context(&vars, "chat");
        assert_eq!(view.len(), 3);
        assert_eq!(get_context_history(&vars, "chat").len(), 3);
    }

    #[test]
    fn default_context_fallback() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "", vec![msg(MessageRole::User, "hi")]);
        assert!(has_context(&vars, DEFAULT_CONTEXT_ID));
        assert_eq!(get_context(&vars, "current").len(), 1);
        assert_eq!(get_context(&vars, "missing").len(), 0);
    }

    #[test]
    fn ledger_accumulates_on_append() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "hello")]);
        append_context(&vars, "chat", vec![msg(MessageRole::User, "world")]);
        // Two appends, two version bumps, estimate covers both messages.
        assert_eq!(array_version(&vars, "chat"), 2);
        assert!(ledger_estimated_tokens(&vars, "chat") > 0);
        assert_eq!(ledger_message_count(&vars, "chat"), 2);
    }

    #[test]
    fn ledger_replacement_marks_dirty_and_recomputes() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "hello world")]);
        let version_before = array_version(&vars, "chat");

        // Replace with a single short message: dirty + version bump.
        register_context(&vars, "chat", vec![msg(MessageRole::Assistant, "ok")]);
        assert!(array_version(&vars, "chat") > version_before);
        // The dirty entry is recomputed on the next read.
        let _ = get_context(&vars, "chat");
        assert_eq!(ledger_message_count(&vars, "chat"), 1);
    }

    #[test]
    fn compression_guard_is_version_keyed() {
        let vars = Arc::new(DashMap::new());
        append_context(&vars, "chat", vec![msg(MessageRole::User, "hi")]);
        let v1 = array_version(&vars, "chat");
        assert!(should_emit_compression(&vars, "chat", v1));
        mark_compression_emitted(&vars, "chat", v1);
        assert!(!should_emit_compression(&vars, "chat", v1));

        // A version change re-arms the guard.
        append_context(&vars, "chat", vec![msg(MessageRole::User, "more")]);
        let v2 = array_version(&vars, "chat");
        assert!(should_emit_compression(&vars, "chat", v2));
    }
}
