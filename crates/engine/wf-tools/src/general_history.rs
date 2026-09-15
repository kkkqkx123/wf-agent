//! History-shape normalization for the `general` proxy boundary.
//!
//! A tool falls into a different bucket (`visible` / `discoverable` /
//! `gated` / `hidden`) on each side of a loop or turn boundary, while stored
//! history still carries the old bucket's call shape: a `general`-wrapped
//! call where the tool is now directly visible, or a direct call where the
//! tool is now only discoverable. The new turn's schema only understands the
//! current shape, so readers of old history see "calls to tools that do not
//! exist".
//!
//! This module offers pure, side-effect-free converters between the two
//! shapes. Stored history (checkpoints, `message_context` archives,
//! `ConversationSession.history()`) is never rewritten in place; conversion
//! applies only at read outlets (a new `AGENT_LOOP`'s initial conversation,
//! optionally a turn projection). Execution semantics never depend on it:
//! the runtime gates stay authoritative over what may execute.
//!
//! Invariants the converters uphold:
//! - Stable, reversible ids: expanded inner ids derive as
//!   `"{outer_id}#{index}#{tool}"`; wrapped proxy ids derive as
//!   `"{first_inner_id}#general"`. No random ids, so checkpoint replay keys
//!   survive conversion.
//! - Order preserving: array order is kept on expand; only consecutive
//!   same-batch direct calls merge on wrap.
//! - Explicit failures: an unparseable `general` body is retained verbatim
//!   (never silently dropped), so the model can still see and correct it.
//! - Idempotent: converting already-normalized history is a no-op under the
//!   same resolution.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use wf_types::message::{LlmFunctionCall, LlmToolCall, Message, MessageContentValue, MessageRole};

use crate::general::GENERAL_TOOL_NAME;
use crate::tool_exposure::ExposureResolution;

/// One inner invocation parsed out of a `general` request body.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedInnerCall {
    pub tool: String,
    /// Serialized JSON object of the inner parameters.
    pub arguments: String,
}

/// Positioned parse failure inside a `general` request body.
#[derive(Debug, Clone, PartialEq)]
pub struct InnerParseError {
    pub index: usize,
    pub reason: String,
}

impl std::fmt::Display for InnerParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invoke item {} is invalid: {}", self.index, self.reason)
    }
}

/// Derive a stable inner call id from its outer proxy call. Deterministic in
/// the outer id, the array position and the tool name, so replays and
/// repeated conversions map to the same key.
pub fn derive_inner_call_id(outer_id: &str, index: usize, tool: &str) -> String {
    format!("{outer_id}#{index}#{tool}")
}

/// Derive a stable proxy call id for a wrapped group. Deterministic in the
/// group's first inner id, so re-wrapping the same group maps to the same
/// key (and a second pass sees the proxy and stops).
pub fn derive_wrapped_call_id(first_inner_id: &str) -> String {
    format!("{first_inner_id}#general")
}

fn is_general_call(call: &LlmToolCall) -> bool {
    call.function.name == GENERAL_TOOL_NAME
}

/// Extract the inner request body from an outer `general` call's arguments.
/// The outer schema carries a single string parameter, so arguments are
/// normally `{"request": "<inner json>"}`; a bare (non-JSON-object)
/// arguments string falls back to itself for leniency towards hand-built
/// histories.
fn outer_request_body(call: &LlmToolCall) -> String {
    let args = call.function.arguments.trim();
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(args) {
        if let Some(Value::String(request)) = map.get("request") {
            return request.clone();
        }
    }
    args.to_string()
}

/// Parse one inner invoke object (`{"tool": ..., "parameters": {...}}`).
fn parse_inner_object(value: Value) -> Result<ParsedInnerCall, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "expected a JSON object".to_string())?;
    let tool = obj
        .get("tool")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required string field \"tool\"".to_string())?;
    if tool.is_empty() {
        return Err("field \"tool\" must not be empty".to_string());
    }
    if tool == GENERAL_TOOL_NAME {
        return Err(format!(
            "tool '{tool}' cannot be invoked through the general tool"
        ));
    }
    let parameters = match obj.get("parameters") {
        Some(Value::Object(_)) => obj
            .get("parameters")
            .expect("object checked above")
            .to_string(),
        Some(_) => return Err("field \"parameters\" must be a JSON object".to_string()),
        None => "{}".to_string(),
    };
    Ok(ParsedInnerCall {
        tool: tool.to_string(),
        arguments: parameters,
    })
}

/// Parse a `general` request body into per-item results. The outer `Err`
/// means the whole body is unusable; per-item `Err`s pinpoint positions.
fn parse_request_items(
    request: &str,
) -> Result<Vec<Result<ParsedInnerCall, InnerParseError>>, String> {
    let trimmed = request.trim();
    if trimmed.is_empty() {
        return Err("empty request body".to_string());
    }
    let value: Value = serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON: {e}"))?;
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                return Err("empty invoke array".to_string());
            }
            Ok(items
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    parse_inner_object(item).map_err(|reason| InnerParseError { index, reason })
                })
                .collect())
        }
        Value::Object(_) => Ok(vec![
            parse_inner_object(value).map_err(|reason| InnerParseError { index: 0, reason })
        ]),
        _ => Err("expected a JSON object or an array of objects".to_string()),
    }
}

fn make_direct_call(id: String, tool: &str, arguments: &str) -> LlmToolCall {
    LlmToolCall {
        id,
        r#type: "function".to_string(),
        function: LlmFunctionCall {
            name: tool.to_string(),
            arguments: arguments.to_string(),
        },
    }
}

fn make_general_call(id: String, request_body: &str) -> LlmToolCall {
    let arguments = serde_json::json!({ "request": request_body }).to_string();
    make_direct_call(id, GENERAL_TOOL_NAME, &arguments)
}

fn single_request_body(tool: &str, arguments: &str) -> String {
    let params: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
    serde_json::json!({ "tool": tool, "parameters": params }).to_string()
}

/// Expand `general`-wrapped calls into direct calls.
///
/// A proxy call expands only when its whole body parses AND every inner
/// tool is in `visible_direct` (the target shape's direct set). Anything
/// else (parse failure, inner tool not directly visible, nested `general`)
/// is retained verbatim so failures stay visible instead of being dropped.
/// Direct calls pass through untouched.
pub fn expand_general_calls_for(
    calls: &[LlmToolCall],
    visible_direct: &HashSet<String>,
) -> Vec<LlmToolCall> {
    let mut out = Vec::with_capacity(calls.len());
    for call in calls {
        if !is_general_call(call) {
            out.push(call.clone());
            continue;
        }
        let body = outer_request_body(call);
        let items = match parse_request_items(&body) {
            Ok(items) => items,
            Err(_) => {
                out.push(call.clone());
                continue;
            }
        };
        let mut parsed = Vec::with_capacity(items.len());
        let mut usable = true;
        for item in items {
            match item {
                Ok(inner) if visible_direct.contains(&inner.tool) => parsed.push(inner),
                _ => {
                    usable = false;
                    break;
                }
            }
        }
        if !usable || parsed.is_empty() {
            out.push(call.clone());
            continue;
        }
        for (index, inner) in parsed.into_iter().enumerate() {
            out.push(make_direct_call(
                derive_inner_call_id(&call.id, index, &inner.tool),
                &inner.tool,
                &inner.arguments,
            ));
        }
    }
    out
}

/// Expand proxy calls without a visibility filter (every well-formed body
/// expands). Prefer [`expand_general_calls_for`] at boundary call sites; this
/// is kept for shape-only rewrites and tests.
pub fn expand_general_calls(calls: &[LlmToolCall]) -> Vec<LlmToolCall> {
    let mut out = Vec::with_capacity(calls.len());
    for call in calls {
        if !is_general_call(call) {
            out.push(call.clone());
            continue;
        }
        let body = outer_request_body(call);
        let items = match parse_request_items(&body) {
            Ok(items) => items,
            Err(_) => {
                out.push(call.clone());
                continue;
            }
        };
        let mut parsed = Vec::with_capacity(items.len());
        let mut usable = true;
        for item in items {
            match item {
                Ok(inner) => parsed.push(inner),
                Err(_) => {
                    usable = false;
                    break;
                }
            }
        }
        if !usable || parsed.is_empty() {
            out.push(call.clone());
            continue;
        }
        for (index, inner) in parsed.into_iter().enumerate() {
            out.push(make_direct_call(
                derive_inner_call_id(&call.id, index, &inner.tool),
                &inner.tool,
                &inner.arguments,
            ));
        }
    }
    out
}

/// Wrap direct calls to currently-discoverable tools into `general` proxy
/// calls. Only consecutive direct calls within the same batch merge; a
/// single-element group still wraps into one single-object proxy call so the
/// target shape is uniform. Proxy calls and non-discoverable directs pass
/// through untouched, which makes the function idempotent.
pub fn wrap_for_discoverable(
    calls: &[LlmToolCall],
    discoverable: &HashSet<String>,
) -> Vec<LlmToolCall> {
    let mut out = Vec::with_capacity(calls.len());
    let mut group: Vec<&LlmToolCall> = Vec::new();
    let flush = |group: &mut Vec<&LlmToolCall>, out: &mut Vec<LlmToolCall>| {
        if group.is_empty() {
            return;
        }
        if group.len() == 1 {
            let inner = group[0];
            let body = single_request_body(&inner.function.name, &inner.function.arguments);
            out.push(make_general_call(derive_wrapped_call_id(&inner.id), &body));
        } else {
            let bodies: Vec<Value> = group
                .iter()
                .map(|inner| {
                    let params: Value =
                        serde_json::from_str(&inner.function.arguments).unwrap_or(Value::Null);
                    serde_json::json!({ "tool": inner.function.name, "parameters": params })
                })
                .collect();
            let body = Value::Array(bodies).to_string();
            out.push(make_general_call(
                derive_wrapped_call_id(&group[0].id),
                &body,
            ));
        }
        group.clear();
    };
    for call in calls {
        if !is_general_call(call) && discoverable.contains(&call.function.name) {
            group.push(call);
        } else {
            flush(&mut group, &mut out);
            out.push(call.clone());
        }
    }
    flush(&mut group, &mut out);
    out
}

fn visible_direct_names(resolution: &ExposureResolution) -> HashSet<String> {
    resolution
        .visible
        .iter()
        .filter(|t| t.name != GENERAL_TOOL_NAME)
        .map(|t| t.name.clone())
        .collect()
}

fn discoverable_names(resolution: &ExposureResolution) -> HashSet<String> {
    resolution
        .discoverable
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

fn message_text(content: &MessageContentValue) -> String {
    match content {
        MessageContentValue::Text(t) => t.clone(),
        // Rich blocks serialize to their JSON form so history normalization
        // never silently drops multi-modal payloads (mirrors the execution
        // path's `content_to_value`). Array payloads still split by position
        // on expand and re-parse on merge; anything else duplicates verbatim.
        MessageContentValue::Rich(blocks) => serde_json::to_string(blocks).unwrap_or_default(),
    }
}

fn tool_message(id: &wf_types::Id, tool_call_id: &str, tool_name: &str, output: &str) -> Message {
    Message {
        id: id.clone(),
        role: MessageRole::Tool,
        content: MessageContentValue::Text(output.to_string()),
        timestamp: wf_common::now(),
        tool_call_id: Some(tool_call_id.to_string()),
        tool_name: Some(tool_name.to_string()),
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

/// Normalize a message history to the target exposure resolution.
///
/// Assistant `tool_calls` arrays convert in both directions: proxy calls
/// whose inners are all currently visible expand to direct calls, and direct
/// calls to currently-discoverable tools merge into proxy calls. Follow-up
/// tool result messages have their `tool_call_id`s remapped to match, so
/// assistant/result pairing never breaks:
/// - expand 1 outer result into N inner results (splitting a JSON-array
///   payload by position when it aligns, otherwise duplicating the payload);
/// - merge N consecutive group results into one proxy result whose payload
///   is the JSON array of the inner payloads.
///
/// All other messages pass through verbatim, and a second run under the same
/// resolution is a no-op.
pub fn normalize_history_for_exposure(
    messages: &[Message],
    resolution: &ExposureResolution,
) -> Vec<Message> {
    let visible = visible_direct_names(resolution);
    let discoverable = discoverable_names(resolution);
    if visible.is_empty() && discoverable.is_empty() {
        return messages.to_vec();
    }

    struct AssistantRewrite {
        calls: Vec<LlmToolCall>,
        /// Outer proxy id -> expanded inner ids (in order).
        expansions: Vec<(String, Vec<(String, String)>)>,
        /// Wrapped proxy id -> member (old id, tool) list (in order).
        wraps: Vec<(String, Vec<(String, String)>)>,
    }

    fn rewrite_calls(
        calls: &[LlmToolCall],
        visible: &HashSet<String>,
        discoverable: &HashSet<String>,
    ) -> AssistantRewrite {
        let mut out = Vec::with_capacity(calls.len());
        let mut expansions = Vec::new();
        let mut wraps = Vec::new();
        let mut pending: Vec<&LlmToolCall> = Vec::new();
        let flush_pending =
            |pending: &mut Vec<&LlmToolCall>,
             out: &mut Vec<LlmToolCall>,
             wraps: &mut Vec<(String, Vec<(String, String)>)>| {
                if pending.is_empty() {
                    return;
                }
                if pending.len() == 1 {
                    let inner = pending[0];
                    let body = single_request_body(&inner.function.name, &inner.function.arguments);
                    let wrapped_id = derive_wrapped_call_id(&inner.id);
                    wraps.push((
                        wrapped_id.clone(),
                        vec![(inner.id.clone(), inner.function.name.clone())],
                    ));
                    out.push(make_general_call(wrapped_id, &body));
                } else {
                    let bodies: Vec<Value> = pending
                        .iter()
                        .map(|inner| {
                            let params: Value = serde_json::from_str(&inner.function.arguments)
                                .unwrap_or(Value::Null);
                            serde_json::json!({ "tool": inner.function.name, "parameters": params })
                        })
                        .collect();
                    let wrapped_id = derive_wrapped_call_id(&pending[0].id);
                    wraps.push((
                        wrapped_id.clone(),
                        pending
                            .iter()
                            .map(|inner| (inner.id.clone(), inner.function.name.clone()))
                            .collect(),
                    ));
                    out.push(make_general_call(
                        wrapped_id,
                        &Value::Array(bodies).to_string(),
                    ));
                }
                pending.clear();
            };
        for call in calls {
            if is_general_call(call) {
                flush_pending(&mut pending, &mut out, &mut wraps);
                let body = outer_request_body(call);
                let expandable = match parse_request_items(&body) {
                    Ok(items) => {
                        let mut parsed = Vec::with_capacity(items.len());
                        let mut ok = true;
                        for item in items {
                            match item {
                                Ok(inner) if visible.contains(&inner.tool) => parsed.push(inner),
                                _ => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok && !parsed.is_empty() {
                            Some(parsed)
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                };
                match expandable {
                    Some(parsed) => {
                        let mut ids = Vec::with_capacity(parsed.len());
                        for (index, inner) in parsed.into_iter().enumerate() {
                            let id = derive_inner_call_id(&call.id, index, &inner.tool);
                            ids.push((id.clone(), inner.tool.clone()));
                            out.push(make_direct_call(id, &inner.tool, &inner.arguments));
                        }
                        expansions.push((call.id.clone(), ids));
                    }
                    None => out.push(call.clone()),
                }
            } else if discoverable.contains(&call.function.name) {
                pending.push(call);
            } else {
                flush_pending(&mut pending, &mut out, &mut wraps);
                out.push(call.clone());
            }
        }
        flush_pending(&mut pending, &mut out, &mut wraps);
        AssistantRewrite {
            calls: out,
            expansions,
            wraps,
        }
    }

    // First pass: rewrite assistant messages, collect id maps.
    let mut rewritten: Vec<Message> = Vec::with_capacity(messages.len());
    let mut expand_index: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut wrap_member_to_proxy: HashMap<String, (String, usize, usize)> = HashMap::new();
    let mut wrap_member_count: HashMap<String, usize> = HashMap::new();
    for msg in messages {
        if msg.role == MessageRole::Assistant {
            if let Some(calls) = msg.tool_calls.as_ref().filter(|c| !c.is_empty()) {
                let rewrite = rewrite_calls(calls, &visible, &discoverable);
                for (outer, inners) in &rewrite.expansions {
                    expand_index.insert(outer.clone(), inners.clone());
                }
                for (proxy_id, members) in &rewrite.wraps {
                    let total = members.len();
                    for (pos, (old_id, _)) in members.iter().enumerate() {
                        wrap_member_to_proxy.insert(old_id.clone(), (proxy_id.clone(), pos, total));
                    }
                    wrap_member_count.insert(proxy_id.clone(), total);
                }
                let mut next = msg.clone();
                next.tool_calls = Some(rewrite.calls);
                rewritten.push(next);
                continue;
            }
        }
        rewritten.push(msg.clone());
    }
    if expand_index.is_empty() && wrap_member_to_proxy.is_empty() {
        return rewritten;
    }

    // Second pass: remap tool result messages.
    let mut out: Vec<Message> = Vec::with_capacity(rewritten.len());
    let mut emitted_wraps: HashSet<String> = HashSet::new();
    // Buffered member results per proxy group; the merged proxy result is
    // emitted at the position of the group's last result.
    let mut pending_wrap: HashMap<String, Vec<Option<Message>>> = wrap_member_count
        .iter()
        .map(|(proxy_id, total)| (proxy_id.clone(), vec![None; *total]))
        .collect();

    for msg in &rewritten {
        if msg.role != MessageRole::Tool {
            out.push(msg.clone());
            continue;
        }
        let Some(call_id) = msg.tool_call_id.as_deref() else {
            out.push(msg.clone());
            continue;
        };
        if let Some(inners) = expand_index.get(call_id) {
            let payload = message_text(&msg.content);
            let split: Option<Vec<String>> = serde_json::from_str::<Value>(&payload)
                .ok()
                .and_then(|v| v.as_array().cloned())
                .filter(|arr| arr.len() == inners.len())
                .map(|arr| {
                    arr.into_iter()
                        .map(|v| match v {
                            Value::String(s) => s,
                            other => other.to_string(),
                        })
                        .collect()
                });
            for (pos, (inner_id, tool)) in inners.iter().enumerate() {
                let text = split
                    .as_ref()
                    .map(|parts| parts[pos].clone())
                    .unwrap_or_else(|| payload.clone());
                out.push(tool_message(&msg.id, inner_id, tool, &text));
            }
            continue;
        }
        if let Some((proxy_id, pos, _)) = wrap_member_to_proxy.get(call_id) {
            if let Some(slots) = pending_wrap.get_mut(proxy_id) {
                slots[*pos] = Some(msg.clone());
                if slots.iter().all(|s| s.is_some()) && !emitted_wraps.contains(proxy_id) {
                    let parts: Vec<Value> = slots
                        .iter()
                        .map(|s| {
                            let raw = s
                                .as_ref()
                                .map(|m| message_text(&m.content))
                                .unwrap_or_default();
                            serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw))
                        })
                        .collect();
                    // Emit the merged proxy result at the position of the
                    // group's last result so earlier messages keep order.
                    out.push(tool_message(
                        &msg.id,
                        proxy_id,
                        GENERAL_TOOL_NAME,
                        &Value::Array(parts).to_string(),
                    ));
                    emitted_wraps.insert(proxy_id.clone());
                }
                // Member results collapse into the merged message; while the
                // group is incomplete the message is held back (buffered).
                continue;
            }
        }
        out.push(msg.clone());
    }
    // Partial histories whose groups never completed would otherwise lose
    // buffered results: re-emit the arrived members verbatim (in group
    // order) so no result is silently dropped.
    for (proxy_id, slots) in &pending_wrap {
        if emitted_wraps.contains(proxy_id) {
            continue;
        }
        for slot in slots.iter().flatten() {
            out.push(slot.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(id: &str, name: &str) -> LlmToolCall {
        make_direct_call(id.to_string(), name, r#"{"q":1}"#)
    }

    fn general(id: &str, request_body: &str) -> LlmToolCall {
        make_general_call(id.to_string(), request_body)
    }

    fn assistant(calls: Vec<LlmToolCall>) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text("working".to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: Some(calls),
            thinking: None,
            metadata: None,
        }
    }

    fn result(call_id: &str, tool: &str, output: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Tool,
            content: MessageContentValue::Text(output.to_string()),
            timestamp: 0,
            tool_call_id: Some(call_id.to_string()),
            tool_name: Some(tool.to_string()),
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn resolution_with(visible: &[&str], discoverable: &[&str]) -> ExposureResolution {
        use wf_types::tool::{Tool, ToolType};
        let tool = |name: &str| Tool {
            id: name.to_string(),
            name: name.to_string(),
            description: name.to_string(),
            tool_type: ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };
        ExposureResolution {
            visible: visible.iter().map(|n| tool(n)).collect(),
            discoverable: discoverable.iter().map(|n| tool(n)).collect(),
            gated: Vec::new(),
            hidden: Vec::new(),
            general_enabled: !discoverable.is_empty(),
        }
    }

    #[test]
    fn inner_ids_are_stable() {
        assert_eq!(derive_inner_call_id("o", 0, "a"), "o#0#a");
        assert_eq!(derive_inner_call_id("o", 0, "a"), "o#0#a");
        assert_ne!(derive_inner_call_id("o", 0, "a"), "o#1#a");
    }

    #[test]
    fn expand_single_proxy_call() {
        let calls = vec![general(
            "outer-1",
            r#"{"tool":"web_search","parameters":{"q":1}}"#,
        )];
        let expanded = expand_general_calls(&calls);
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].function.name, "web_search");
        assert_eq!(expanded[0].id, "outer-1#0#web_search");
    }

    #[test]
    fn expand_multi_preserves_order() {
        let calls = vec![general(
            "outer-1",
            r#"[{"tool":"a","parameters":{}},{"tool":"b","parameters":{}}]"#,
        )];
        let expanded = expand_general_calls(&calls);
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].id, "outer-1#0#a");
        assert_eq!(expanded[1].id, "outer-1#1#b");
    }

    #[test]
    fn expand_keeps_illegal_body_verbatim() {
        let calls = vec![
            general("outer-bad", "not json"),
            general("outer-partial", r#"[{"tool":"a"},{"no_tool":1}]"#),
        ];
        let expanded = expand_general_calls(&calls);
        assert_eq!(expanded.len(), 2);
        assert!(expanded
            .iter()
            .all(|c| c.function.name == GENERAL_TOOL_NAME));
    }

    #[test]
    fn expand_respects_visibility_filter() {
        let calls = vec![general(
            "outer-1",
            r#"{"tool":"web_search","parameters":{}}"#,
        )];
        let visible: HashSet<String> = ["read_file".to_string()].into_iter().collect();
        let kept = expand_general_calls_for(&calls, &visible);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].function.name, GENERAL_TOOL_NAME);
    }

    #[test]
    fn wrap_groups_consecutive_discoverable() {
        let discoverable: HashSet<String> =
            ["a".to_string(), "b".to_string()].into_iter().collect();
        let calls = vec![direct("1", "a"), direct("2", "b"), direct("3", "read")];
        let wrapped = wrap_for_discoverable(&calls, &discoverable);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0].function.name, GENERAL_TOOL_NAME);
        assert_eq!(wrapped[0].id, "1#general");
        assert_eq!(wrapped[1].function.name, "read");
        // Idempotent: wrapping again is a no-op.
        let again = wrap_for_discoverable(&wrapped, &discoverable);
        assert_eq!(again.len(), 2);
        assert_eq!(again[0].id, wrapped[0].id);
    }

    #[test]
    fn normalize_expands_and_remaps_results() {
        let resolution = resolution_with(&["web_search", "general"], &[]);
        let history = vec![
            assistant(vec![general(
                "outer-1",
                r#"{"tool":"web_search","parameters":{"q":1}}"#,
            )]),
            result("outer-1", "general", r#"{"answer":42}"#),
        ];
        let normalized = normalize_history_for_exposure(&history, &resolution);
        assert_eq!(normalized.len(), 2);
        let calls = normalized[0].tool_calls.as_ref().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "outer-1#0#web_search");
        assert_eq!(
            normalized[1].tool_call_id.as_deref(),
            Some("outer-1#0#web_search")
        );
        // Idempotent under the same resolution.
        let again = normalize_history_for_exposure(&normalized, &resolution);
        assert_eq!(again, normalized);
    }

    #[test]
    fn normalize_wraps_and_merges_results() {
        let resolution = resolution_with(&["general"], &["web_search"]);
        let history = vec![
            assistant(vec![direct("c-1", "web_search")]),
            result("c-1", "web_search", r#"{"answer":1}"#),
        ];
        let normalized = normalize_history_for_exposure(&history, &resolution);
        assert_eq!(normalized.len(), 2);
        let calls = normalized[0].tool_calls.as_ref().expect("calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, GENERAL_TOOL_NAME);
        assert_eq!(
            normalized[1].tool_call_id.as_deref(),
            Some(calls[0].id.as_str())
        );
        let again = normalize_history_for_exposure(&normalized, &resolution);
        assert_eq!(again, normalized);
    }
}
