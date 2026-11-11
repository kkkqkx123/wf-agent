//! Session tree browser: a read-only navigable tree of one session.
//!
//! The trunk is linear (conversation messages oldest-first with checkpoint
//! section markers interleaved by timestamp), which is all the server model
//! offers: message-level branches do not exist, so there is no pi-style leaf
//! pointer to move. Checkpoint headers own foldable sections; the nearest
//! checkpoint at-or-before each node is recorded so phase 2 can branch from
//! any selected node via checkpoint restore into a new tab.
//!
//! All assembly, filtering, searching, folding and selection-keep rules are
//! pure functions over plain data so they unit-test without a client.

use wf_types::message::{Message, MessageContent, MessageContentValue, MessageRole};

/// Upper bound on assembled nodes; the newest messages win and the status
/// line reports the truncation. Keeps opens of huge sessions interactive.
pub const SESSION_TREE_NODE_CAP: usize = 1500;

/// Display width budget for one assembled row. Rows are single-line
/// summaries pre-truncated here; the draw layer additionally clips to the
/// popup width, so no horizontal panning is needed.
pub const SESSION_TREE_ROW_CHARS: usize = 160;

/// One row of the session tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Message id, or `checkpoint:<id>` for section headers.
    pub id: String,
    pub kind: TreeNodeKind,
    /// Single-line display text, pre-truncated to [`SESSION_TREE_ROW_CHARS`].
    pub text: String,
    /// Full text behind the row, used for per-row copy.
    pub full_text: String,
    /// Unix milliseconds; orders headers among messages.
    pub timestamp: i64,
    /// Nearest checkpoint at-or-before this node; the phase 2 branch source.
    pub checkpoint_id: Option<String>,
    /// Tool name for tool rows, used by search.
    pub tool_name: Option<String>,
    /// Tool success for tool rows, when known.
    pub success: Option<bool>,
}

/// Row kinds. `System` rows are bookkeeping and hidden outside `All`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeNodeKind {
    User,
    Assistant,
    Tool,
    Checkpoint,
    Summary,
    System,
}

impl TreeNodeKind {
    /// Leading glyph, reusing the transcript vocabulary.
    pub fn glyph(self) -> &'static str {
        match self {
            TreeNodeKind::User => "❯",
            TreeNodeKind::Assistant => "◆",
            TreeNodeKind::Tool => "▲",
            TreeNodeKind::Checkpoint => "⬢",
            TreeNodeKind::Summary => "▦",
            TreeNodeKind::System => "·",
        }
    }
}

/// View filter for the tree browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeFilter {
    /// Hide bookkeeping rows (system messages).
    #[default]
    Default,
    /// Default minus tool rows.
    NoTools,
    /// Show everything.
    All,
    /// Only client-side bookmarked rows.
    Bookmarked,
}

impl TreeFilter {
    pub fn label(self) -> &'static str {
        match self {
            TreeFilter::Default => "default",
            TreeFilter::NoTools => "no-tools",
            TreeFilter::All => "all",
            TreeFilter::Bookmarked => "marked",
        }
    }

    /// Cycle forward: default -> no-tools -> all -> marked -> default.
    pub fn cycle(self) -> TreeFilter {
        match self {
            TreeFilter::Default => TreeFilter::NoTools,
            TreeFilter::NoTools => TreeFilter::All,
            TreeFilter::All => TreeFilter::Bookmarked,
            TreeFilter::Bookmarked => TreeFilter::Default,
        }
    }

    pub(crate) fn keeps(self, kind: TreeNodeKind) -> bool {
        match self {
            TreeFilter::Default => kind != TreeNodeKind::System,
            TreeFilter::NoTools => kind != TreeNodeKind::System && kind != TreeNodeKind::Tool,
            // Bookmarked narrows by bookmark set in the visible pass, not
            // by kind, so it keeps every kind here.
            TreeFilter::All | TreeFilter::Bookmarked => true,
        }
    }
}

/// One checkpoint point extracted from a chain response.
#[derive(Debug, Clone)]
pub struct CheckpointPoint {
    pub id: String,
    pub timestamp: i64,
}

/// Flatten a checkpoint chain response into points. Both the agent-loop
/// capped item view (`items` of checkpoints) and the execution chain
/// analysis shape (chains of nested `checkpoints` arrays) fold into one
/// timestamped pool; callers decide by timestamp, never by position.
pub fn checkpoint_points(value: &serde_json::Value) -> Vec<CheckpointPoint> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| value.as_array().cloned())
        .unwrap_or_default();
    let mut flat: Vec<serde_json::Value> = Vec::new();
    for item in items {
        if let Some(nested) = item.get("checkpoints").and_then(|v| v.as_array()) {
            flat.extend(nested.iter().cloned());
        } else {
            flat.push(item);
        }
    }
    flat.iter()
        .filter_map(|cp| {
            Some(CheckpointPoint {
                id: cp.get("id")?.as_str()?.to_string(),
                timestamp: cp.get("timestamp")?.as_i64()?,
            })
        })
        .collect()
}

/// Plain-text content of a message, skipping tool/image payloads.
/// Image blocks collapse to a placeholder so image-carrying turns stay
/// visible where no image renderer exists.
fn message_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => parts
            .iter()
            .filter_map(|part| match part {
                MessageContent::Text { text } => Some(text.clone()),
                MessageContent::Thinking { thinking, .. } => Some(thinking.clone()),
                MessageContent::ToolResult { tool_result } => Some(tool_result.content.clone()),
                MessageContent::ImageUrl { image_url } => {
                    Some(wf_types::message::image_block_label(image_url))
                }
                MessageContent::ToolUse { .. }
                | MessageContent::InputAudio { .. }
                | MessageContent::FileInput { .. }
                | MessageContent::RedactedThinking { .. }
                | MessageContent::Native { .. } => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Declared tool calls on the message envelope plus bare `ToolUse` parts.
/// Returns `(name, arguments preview, success)` per call.
fn tool_calls_of(message: &Message) -> Vec<(String, String, Option<bool>)> {
    let mut calls: Vec<(String, String, Option<bool>)> = Vec::new();
    if let Some(declared) = &message.tool_calls {
        for call in declared {
            calls.push((
                call.function.name.clone(),
                call.function.arguments.clone(),
                None,
            ));
        }
    }
    if let MessageContentValue::Rich(parts) = &message.content {
        for part in parts {
            match part {
                MessageContent::ToolUse { tool_use } => {
                    if calls.iter().any(|(name, _, _)| name == &tool_use.name) {
                        continue;
                    }
                    let preview = tool_use.input.to_string();
                    calls.push((tool_use.name.clone(), preview, None));
                }
                MessageContent::ToolResult { tool_result } => {
                    let success = tool_result.is_error.map(|error| !error);
                    if let Some((_, _, slot)) = calls.iter_mut().find(|(_, _, slot)| slot.is_none())
                    {
                        let _ = slot.insert(success.unwrap_or(true));
                    }
                }
                _ => {}
            }
        }
    }
    // Envelope metadata success marker, as a last resort.
    if let Some(metadata) = message.metadata.as_ref() {
        if let Some(success) = metadata.get("success").and_then(|v| v.as_bool()) {
            for (_, _, slot) in calls.iter_mut() {
                if slot.is_none() {
                    *slot = Some(success);
                }
            }
        }
    }
    calls
}

/// Success signal for a tool-role message.
fn tool_message_success(message: &Message, text: &str) -> bool {
    if let MessageContentValue::Rich(parts) = &message.content {
        for part in parts {
            if let MessageContent::ToolResult { tool_result } = part {
                if let Some(is_error) = tool_result.is_error {
                    return !is_error;
                }
            }
        }
    }
    if let Some(metadata) = message.metadata.as_ref() {
        if let Some(success) = metadata.get("success").and_then(|v| v.as_bool()) {
            return success;
        }
    }
    let lower = text.to_lowercase();
    !(lower.contains("error") || lower.contains("failed"))
}

/// First line of `text`, whitespace-folded and truncated to the row budget.
fn first_line(text: &str) -> String {
    let folded: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = folded.chars().take(SESSION_TREE_ROW_CHARS).collect();
    if folded.chars().count() > SESSION_TREE_ROW_CHARS {
        out.push('…');
    }
    out
}

/// Short id for checkpoint headers.
fn short_id(id: &str) -> String {
    const KEEP: usize = 8;
    if id.len() <= KEEP {
        id.to_string()
    } else {
        id.chars().take(KEEP).collect()
    }
}

/// Assemble the display tree: messages oldest-first with checkpoint section
/// headers interleaved by timestamp, each node carrying its nearest
/// at-or-before checkpoint. Iteration details back the trunk only when no
/// messages were persisted. The newest `SESSION_TREE_NODE_CAP` messages win;
/// `truncated` reports how many oldest messages were dropped.
pub fn assemble_tree(
    messages: &[Message],
    checkpoints: &[CheckpointPoint],
    iterations: &[serde_json::Value],
) -> (Vec<TreeNode>, bool) {
    let mut nodes: Vec<TreeNode> = Vec::new();
    let mut dropped = 0usize;
    if messages.is_empty() {
        for detail in iterations {
            if let Some(content) = detail.get("response_content").and_then(|v| v.as_str()) {
                if !content.trim().is_empty() {
                    nodes.push(TreeNode {
                        id: format!(
                            "iteration:{}",
                            detail
                                .get("iteration")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                        ),
                        kind: TreeNodeKind::Assistant,
                        text: first_line(content),
                        full_text: content.to_string(),
                        timestamp: detail
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        checkpoint_id: None,
                        tool_name: None,
                        success: None,
                    });
                }
            }
            if let Some(calls) = detail.get("tool_calls").and_then(|v| v.as_array()) {
                for call in calls {
                    let name = call
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let success = call
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    nodes.push(TreeNode {
                        id: format!("iteration-tool:{name}"),
                        kind: TreeNodeKind::Tool,
                        text: name.clone(),
                        full_text: name.clone(),
                        timestamp: detail
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        checkpoint_id: None,
                        tool_name: Some(name),
                        success: Some(success),
                    });
                }
            }
        }
    } else {
        let kept: &[Message] = if messages.len() > SESSION_TREE_NODE_CAP {
            dropped = messages.len() - SESSION_TREE_NODE_CAP;
            &messages[messages.len() - SESSION_TREE_NODE_CAP..]
        } else {
            messages
        };
        for message in kept {
            let text = message_text(message);
            match message.role {
                MessageRole::User => {
                    if text.trim().is_empty() {
                        continue;
                    }
                    nodes.push(TreeNode {
                        id: message.id.clone(),
                        kind: TreeNodeKind::User,
                        text: first_line(&text),
                        full_text: text,
                        timestamp: message.timestamp,
                        checkpoint_id: None,
                        tool_name: None,
                        success: None,
                    });
                }
                MessageRole::Assistant => {
                    if !text.trim().is_empty() {
                        nodes.push(TreeNode {
                            id: message.id.clone(),
                            kind: TreeNodeKind::Assistant,
                            text: first_line(&text),
                            full_text: text,
                            timestamp: message.timestamp,
                            checkpoint_id: None,
                            tool_name: None,
                            success: None,
                        });
                    }
                    for (name, arguments, success) in tool_calls_of(message) {
                        let preview: String = arguments.chars().take(80).collect();
                        nodes.push(TreeNode {
                            id: format!("{}:{name}", message.id),
                            kind: TreeNodeKind::Tool,
                            text: if preview.trim().is_empty() {
                                name.clone()
                            } else {
                                format!("{name} {preview}")
                            },
                            full_text: format!("{name} {arguments}"),
                            timestamp: message.timestamp,
                            checkpoint_id: None,
                            tool_name: Some(name),
                            success,
                        });
                    }
                }
                MessageRole::Tool => {
                    let name = message
                        .tool_name
                        .clone()
                        .unwrap_or_else(|| "tool".to_string());
                    let success = tool_message_success(message, &text);
                    let body = if text.trim().is_empty() {
                        name.clone()
                    } else {
                        format!("{name}: {}", first_line(&text))
                    };
                    nodes.push(TreeNode {
                        id: message.id.clone(),
                        kind: TreeNodeKind::Tool,
                        text: body,
                        full_text: format!("{name}\n{text}"),
                        timestamp: message.timestamp,
                        checkpoint_id: None,
                        tool_name: Some(name),
                        success: Some(success),
                    });
                }
                MessageRole::System => {
                    if text.trim().is_empty() {
                        continue;
                    }
                    nodes.push(TreeNode {
                        id: message.id.clone(),
                        kind: TreeNodeKind::System,
                        text: first_line(&text),
                        full_text: text,
                        timestamp: message.timestamp,
                        checkpoint_id: None,
                        tool_name: None,
                        success: None,
                    });
                }
            }
        }
    }
    // Checkpoint section headers interleave by timestamp; messages win ties
    // so a checkpoint taken after a turn renders below that turn.
    for point in checkpoints {
        // Skip checkpoints far older than a truncated trunk, but keep the
        // nearest older one as the section anchor for the kept range.
        nodes.push(TreeNode {
            id: format!("checkpoint:{}", point.id),
            kind: TreeNodeKind::Checkpoint,
            text: format!("checkpoint {}", short_id(&point.id)),
            full_text: format!("checkpoint {}", point.id),
            timestamp: point.timestamp,
            checkpoint_id: Some(point.id.clone()),
            tool_name: None,
            success: None,
        });
    }
    nodes.sort_by(|a, b| {
        a.timestamp.cmp(&b.timestamp).then_with(|| {
            let rank = |node: &TreeNode| u8::from(node.kind == TreeNodeKind::Checkpoint);
            rank(a).cmp(&rank(b))
        })
    });
    // Nearest at-or-before checkpoint per node.
    let mut current: Option<String> = None;
    for node in nodes.iter_mut() {
        if node.kind == TreeNodeKind::Checkpoint {
            current = node.checkpoint_id.clone();
        } else if current.is_some() {
            node.checkpoint_id = current.clone();
        }
    }
    (nodes, dropped > 0)
}

/// Assembled tree payload crossing the fetch task boundary.
#[derive(Debug)]
pub struct AssembledTree {
    pub nodes: Vec<TreeNode>,
    pub truncated: bool,
    pub total_messages: usize,
}

/// Scope of a checkpoint restore triggered from the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreScope {
    /// Restore execution/agent-loop state only, into a new tab.
    History,
    /// Restore the linked workspace files only.
    Workspace,
    /// Restore both state and workspace files.
    Both,
}

mod view;

pub use view::SessionTree;

#[cfg(test)]
mod test;
