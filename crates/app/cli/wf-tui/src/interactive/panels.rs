//! Footer panel wiring for the interactive session.
//!
//! The panels themselves are pure components in `tui-components`; this
//! module owns the session side: which route is open, where the candidates
//! come from and what a selection does.
//!
//! Two input ownership modes exist:
//!
//! * exclusive routes (command / model / skill / queued / workflow) own the
//!   text input: printable characters feed the panel filter and the composer
//!   is paused;
//! * the mention route is composite: the composer keeps the text and the
//!   panel is a derived view over the mention query under the cursor.

use crate::footer::FooterRoute;
use crate::interactive::{
    InteractiveAction, InteractiveController, InteractiveEvent, PanelPayload,
};
use crate::keymap::{CKey, Key};
use tui_components::footer::PanelState;
use tui_components::panels::{
    AuthPanel, AuthPromptOption, AuthSessionView, AuthSurfaceRow, CommandPalette, MentionEntry,
    MentionPanel, ModelPanel, QueuedPanel, SkillPanel, WorkflowPanel, WorkflowSummaryRow,
};
use tui_core::command::{self, CommandRegistry, CommandSpec, RuntimeCommand};
use tui_core::keymap::KeyAction;

/// Entity kinds requested from the mention search endpoint. Files are not
/// requested: the local scan already covers them.
const MENTION_ENTITY_KINDS: &str =
    "skill,tool,script,resource,workflow,agent,agent_loop,execution,checkpoint,task,event,message";

/// Upper bound of the mention search page size.
const MENTION_SEARCH_LIMIT: usize = 40;

/// Seconds between auth login-session polls when the server does not
/// advertise a device-code interval (RFC 8628 default).
const AUTH_POLL_FALLBACK_SECS: u64 = 5;

/// Items of a list response (bare array or page shell).
fn items_of(data: &serde_json::Value) -> Vec<serde_json::Value> {
    data.get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| data.as_array().cloned())
        .unwrap_or_default()
}

/// Parse one model profile from the profiles response.
fn parse_profile(value: &serde_json::Value) -> Option<wf_types::llm::LlmProfile> {
    serde_json::from_value::<wf_types::llm::LlmProfile>(value.clone()).ok()
}

/// Parse the mention candidates into panel entries.
fn parse_mention_candidates(value: &serde_json::Value) -> Vec<MentionEntry> {
    items_of(value)
        .iter()
        .filter_map(|item| {
            let kind = item.get("kind")?.as_str()?.to_string();
            let value = item
                .get("value")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("id").and_then(|v| v.as_str()))
                .unwrap_or_default()
                .to_string();
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let mut entry = MentionEntry::new(kind, value);
            if let Some(description) = description {
                entry = entry.described(description);
            }
            Some(entry)
        })
        .collect()
}

/// Percent-encode a query fragment for the mention search endpoint.
fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Split an optional leading `--node <id>` / `--node=<id>` flag from slash
/// arguments. The flag selects a nested workflow node explicitly; its
/// absence means the selected nested target (or the loop-only path) applies.
fn split_node_flag(argument: &str) -> (Option<String>, Vec<String>) {
    let mut tokens: Vec<String> = argument.split_whitespace().map(str::to_string).collect();
    let node = if tokens.first().map(String::as_str) == Some("--node") {
        tokens.remove(0);
        if tokens.is_empty() {
            return (None, Vec::new());
        }
        Some(tokens.remove(0))
    } else if let Some(id) = tokens
        .first()
        .and_then(|first| first.strip_prefix("--node="))
        .map(str::to_string)
    {
        tokens.remove(0);
        if id.trim().is_empty() {
            None
        } else {
            Some(id)
        }
    } else {
        None
    };
    (node, tokens)
}

/// Visible rows of a reply, folding long lines to the terminal width.
fn reply_rows(text: &str) -> usize {
    let cols = crossterm::terminal::size()
        .map(|(cols, _)| cols)
        .unwrap_or(80)
        .max(20) as usize;
    text.lines()
        .map(|line| {
            let width = line.chars().count().max(1);
            width.div_ceil(cols).max(1)
        })
        .sum::<usize>()
        .max(1)
}

mod auth;
mod candidates;
mod panel_command;
mod panel_keys;
mod plugin;
mod recap;

#[cfg(test)]
mod test;
