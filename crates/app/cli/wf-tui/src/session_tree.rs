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

use std::collections::HashSet;

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

    fn keeps(self, kind: TreeNodeKind) -> bool {
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
fn message_text(message: &Message) -> String {
    match &message.content {
        MessageContentValue::Text(text) => text.clone(),
        MessageContentValue::Rich(parts) => parts
            .iter()
            .filter_map(|part| match part {
                MessageContent::Text { text } => Some(text.clone()),
                MessageContent::Thinking { thinking, .. } => Some(thinking.clone()),
                MessageContent::ToolResult { tool_result } => Some(tool_result.content.clone()),
                MessageContent::ToolUse { .. }
                | MessageContent::ImageUrl { .. }
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

/// Session tree browser state: nodes plus view controls. The visible list is
/// derived (filter -> search -> fold) and the selection survives view changes
/// by node id, falling back to the nearest following row.
#[derive(Debug, Default)]
pub struct SessionTree {
    session_id: Option<String>,
    nodes: Vec<TreeNode>,
    visible: Vec<usize>,
    selected: usize,
    selected_id: Option<String>,
    filter: TreeFilter,
    search: String,
    folded: HashSet<String>,
    /// Client-side bookmarked node ids for this session. Set by the shell
    /// from the persisted bookmarks file; stale ids simply match nothing.
    bookmarks: HashSet<String>,
    /// Whether the restore-scope selector is open on top of the tree.
    restore_prompt: bool,
    truncated: bool,
    total_messages: usize,
    loading: bool,
    error: Option<String>,
}

impl SessionTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the browser for `session_id`, dropping whatever was loaded.
    pub fn open(&mut self, session_id: String) {
        self.session_id = Some(session_id);
        self.nodes.clear();
        self.visible.clear();
        self.selected = 0;
        self.selected_id = None;
        self.filter = TreeFilter::Default;
        self.search.clear();
        self.folded.clear();
        self.bookmarks.clear();
        self.restore_prompt = false;
        self.truncated = false;
        self.total_messages = 0;
        self.loading = true;
        self.error = None;
    }

    /// Session the open browser belongs to, if any.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn filter(&self) -> TreeFilter {
        self.filter
    }

    pub fn search_query(&self) -> &str {
        &self.search
    }

    /// Land fetched nodes. The trunk stays empty with an error note when the
    /// fetch failed; an empty trunk is a real (fresh session) state.
    pub fn land(&mut self, nodes: Vec<TreeNode>, truncated: bool, total_messages: usize) {
        self.loading = false;
        self.error = None;
        self.nodes = nodes;
        self.truncated = truncated;
        self.total_messages = total_messages;
        // Start at the live tail like the reference, which opens on the
        // current leaf; filters and search move the cursor from there.
        self.selected_id = self.nodes.last().map(|node| node.id.clone());
        self.rebuild_visible();
    }

    /// Stop loading after a failed fetch, keeping the reason for display.
    pub fn fail(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
        self.rebuild_visible();
    }

    /// Close the browser and forget its session.
    pub fn close(&mut self) {
        self.session_id = None;
        self.nodes.clear();
        self.visible.clear();
        self.selected = 0;
        self.selected_id = None;
        self.search.clear();
        self.folded.clear();
        self.restore_prompt = false;
        self.loading = false;
        self.error = None;
    }

    /// Number of visible rows.
    pub fn len(&self) -> usize {
        self.visible.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Selected node, if any.
    pub fn selected_node(&self) -> Option<&TreeNode> {
        self.visible
            .get(self.selected)
            .and_then(|&i| self.nodes.get(i))
    }

    /// Full text behind the selection, for per-row copy.
    pub fn selected_copy_text(&self) -> Option<String> {
        self.selected_node().map(|node| node.full_text.clone())
    }

    /// Replace the bookmarked node ids for this session.
    pub fn set_bookmarks(&mut self, ids: Vec<String>) {
        self.bookmarks = ids.into_iter().collect();
        if self.filter == TreeFilter::Bookmarked {
            self.rebuild_visible();
        }
    }

    /// Toggle the bookmark on the selected node. Returns the node id, a
    /// label snippet and whether it is now marked; the shell persists it.
    pub fn toggle_bookmark_selected(&mut self) -> Option<(String, String, bool)> {
        let node = self.selected_node()?;
        let id = node.id.clone();
        let label = node.text.clone();
        let marked = if self.bookmarks.contains(&id) {
            self.bookmarks.remove(&id);
            false
        } else {
            self.bookmarks.insert(id.clone());
            true
        };
        if self.filter == TreeFilter::Bookmarked {
            self.rebuild_visible();
        }
        Some((id, label, marked))
    }

    /// Display rows with fold and live-tail markers applied. The newest node
    /// carries the live-position marker.
    pub fn rows(&self) -> Vec<String> {
        let last = self.visible.len().checked_sub(1);
        self.visible
            .iter()
            .enumerate()
            .map(|(pos, &i)| {
                let node = &self.nodes[i];
                let mut row = String::new();
                if node.kind == TreeNodeKind::Checkpoint {
                    let folded = self.folded.contains(&node.id);
                    row.push_str(if folded { "▸ " } else { "▾ " });
                } else {
                    row.push_str("  ");
                }
                row.push_str(node.kind.glyph());
                row.push(' ');
                if self.bookmarks.contains(&node.id) {
                    row.push_str("★ ");
                }
                row.push_str(&node.text);
                if Some(pos) == last {
                    row.push_str(" ●");
                }
                row
            })
            .collect()
    }

    /// Status line: position, filter, search and fold/truncation notes.
    pub fn status_line(&self) -> String {
        if self.visible.is_empty() {
            return format!("(0/0) [{}]", self.filter.label());
        }
        let mut parts = vec![
            format!("({}/{})", self.selected + 1, self.visible.len()),
            format!("[{}]", self.filter.label()),
        ];
        if !self.search.is_empty() {
            parts.push(format!("search: {}", self.search));
        }
        if let Some(node) = self.selected_node() {
            if let Some(cp) = &node.checkpoint_id {
                parts.push(format!("cp: {}", &cp[..cp.len().min(8)]));
            }
        }
        if !self.folded.is_empty() {
            parts.push(format!("folded: {}", self.folded.len()));
        }
        if !self.bookmarks.is_empty() {
            parts.push(format!("marked: {}", self.bookmarks.len()));
        }
        if self.truncated {
            parts.push(format!(
                "showing newest {}/{}",
                self.nodes.len(),
                self.total_messages
            ));
        }
        parts.join(" · ")
    }

    /// Move the selection, wrapping around the visible list.
    pub fn move_next(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.visible.len();
        self.remember_selection();
    }

    pub fn move_prev(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.visible.len() - 1
        } else {
            self.selected - 1
        };
        self.remember_selection();
    }

    /// Page the selection, clamping at the ends.
    pub fn page_by(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, self.visible.len() as isize - 1) as usize;
        self.remember_selection();
    }

    pub fn jump_to_first(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = 0;
        self.remember_selection();
    }

    pub fn jump_to_last(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        self.selected = self.visible.len() - 1;
        self.remember_selection();
    }

    /// Set the filter; clears folds like the reference so a fresh view never
    /// hides rows behind a stale fold.
    pub fn set_filter(&mut self, filter: TreeFilter) {
        self.filter = filter;
        self.folded.clear();
        self.rebuild_visible();
    }

    pub fn cycle_filter(&mut self) {
        let next = self.filter.cycle();
        self.set_filter(next);
    }

    /// Push a search character; clears folds so typing never hides matches
    /// behind a stale fold.
    pub fn search_push(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.search.push(c);
        self.folded.clear();
        self.rebuild_visible();
    }

    pub fn search_backspace(&mut self) {
        self.search.pop();
        self.folded.clear();
        self.rebuild_visible();
    }

    pub fn clear_search(&mut self) {
        if self.search.is_empty() {
            return;
        }
        self.search.clear();
        self.folded.clear();
        self.rebuild_visible();
    }

    /// Fold or unfold the selected checkpoint section. Non-checkpoint rows
    /// unfold their enclosing section instead.
    pub fn toggle_fold(&mut self) {
        let section = self.selected_section_id();
        let Some(section) = section else {
            return;
        };
        if self.folded.contains(&section) {
            self.folded.remove(&section);
        } else {
            self.folded.insert(section);
        }
        self.rebuild_visible();
    }

    /// Whether the restore-scope selector is currently open.
    pub fn restore_prompt(&self) -> bool {
        self.restore_prompt
    }

    /// Open the restore-scope selector on the current selection.
    pub fn open_restore_prompt(&mut self) {
        if self
            .selected_node()
            .and_then(|n| n.checkpoint_id.as_ref())
            .is_some()
        {
            self.restore_prompt = true;
        }
    }

    /// Close the restore-scope selector without touching the tree.
    pub fn close_restore_prompt(&mut self) {
        self.restore_prompt = false;
    }

    /// Expand every checkpoint section.
    pub fn expand_all(&mut self) {
        self.folded.clear();
        self.rebuild_visible();
    }

    /// Collapse every checkpoint section.
    pub fn collapse_all(&mut self) {
        self.folded = self
            .nodes
            .iter()
            .filter(|n| n.kind == TreeNodeKind::Checkpoint)
            .map(|n| n.id.clone())
            .collect();
        self.rebuild_visible();
    }

    /// Whether the selected row can fold or unfold something.
    pub fn can_fold(&self) -> bool {
        self.selected_section_id().is_some()
    }

    /// Checkpoint section owning the selection: the selected header itself,
    /// or the nearest header above a message row.
    fn selected_section_id(&self) -> Option<String> {
        let &index = self.visible.get(self.selected)?;
        if self.nodes.get(index)?.kind == TreeNodeKind::Checkpoint {
            return Some(self.nodes[index].id.clone());
        }
        // Walk back to the nearest checkpoint header above the row.
        let mut cursor = self.visible.iter().position(|&i| i == index)?;
        while cursor > 0 {
            cursor -= 1;
            let node = &self.nodes[self.visible[cursor]];
            if node.kind == TreeNodeKind::Checkpoint {
                // A section is foldable only when it actually owns rows.
                return Some(node.id.clone());
            }
        }
        None
    }

    fn remember_selection(&mut self) {
        self.selected_id = self
            .visible
            .get(self.selected)
            .and_then(|&i| self.nodes.get(i))
            .map(|node| node.id.clone());
    }

    /// Rebuild the visible list (filter -> search -> fold) and restore the
    /// selection: same node id when still visible, else the nearest row
    /// at-or-after the old position, else the last row.
    fn rebuild_visible(&mut self) {
        let tokens: Vec<String> = self
            .search
            .to_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let mut kept: Vec<usize> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            if !self.filter.keeps(node.kind) {
                continue;
            }
            if self.filter == TreeFilter::Bookmarked && !self.bookmarks.contains(&node.id) {
                continue;
            }
            if !tokens.is_empty() {
                let haystack = format!("{} {}", node.text, node.tool_name.as_deref().unwrap_or(""))
                    .to_lowercase();
                if !tokens.iter().all(|token| haystack.contains(token)) {
                    continue;
                }
            }
            kept.push(index);
        }
        // Drop rows hidden under folded checkpoint sections. A section spans
        // from its header to the next header; headers always stay visible.
        if !self.folded.is_empty() {
            let mut folded_from: Option<usize> = None;
            let mut shown: Vec<usize> = Vec::new();
            for index in kept {
                let node = &self.nodes[index];
                if node.kind == TreeNodeKind::Checkpoint {
                    folded_from = if self.folded.contains(&node.id) {
                        Some(index)
                    } else {
                        None
                    };
                    shown.push(index);
                } else if folded_from.is_none() {
                    shown.push(index);
                }
            }
            kept = shown;
        }
        self.visible = kept;
        if self.visible.is_empty() {
            self.selected = 0;
            return;
        }
        if let Some(id) = self.selected_id.clone() {
            if let Some(pos) = self
                .visible
                .iter()
                .position(|&i| self.nodes.get(i).is_some_and(|node| node.id == id))
            {
                self.selected = pos;
                return;
            }
        }
        self.selected = self.selected.min(self.visible.len() - 1);
        self.remember_selection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::MessageRole;

    fn user_message(id: &str, text: &str, timestamp: i64) -> Message {
        Message {
            id: id.to_string(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    fn assistant_message(id: &str, text: &str, timestamp: i64) -> Message {
        Message {
            id: id.to_string(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text(text.to_string()),
            timestamp,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    fn open_tree() -> SessionTree {
        let mut tree = SessionTree::new();
        tree.open("exec-1".to_string());
        tree
    }

    fn land_sample(tree: &mut SessionTree) {
        let messages = vec![
            user_message("u1", "read the file", 1000),
            assistant_message("a1", "reading now", 2000),
            user_message("u2", "run the tests", 3000),
        ];
        let checkpoints = vec![CheckpointPoint {
            id: "cp-1".to_string(),
            timestamp: 1500,
        }];
        let (nodes, truncated) = assemble_tree(&messages, &checkpoints, &[]);
        tree.land(nodes, truncated, messages.len());
    }

    #[test]
    fn open_replaces_the_session_and_close_forgets_it() {
        let mut tree = SessionTree::new();
        assert_eq!(tree.session_id(), None);
        tree.open("exec-1".to_string());
        assert_eq!(tree.session_id(), Some("exec-1"));
        tree.open("exec-2".to_string());
        assert_eq!(tree.session_id(), Some("exec-2"));
        tree.close();
        assert_eq!(tree.session_id(), None);
    }

    #[test]
    fn assemble_interleaves_checkpoints_and_records_nearest() {
        let messages = vec![
            user_message("u1", "hello", 1000),
            assistant_message("a1", "hi", 2000),
        ];
        let checkpoints = vec![CheckpointPoint {
            id: "cp-1".to_string(),
            timestamp: 1500,
        }];
        let (nodes, truncated) = assemble_tree(&messages, &checkpoints, &[]);
        assert!(!truncated);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].id, "u1");
        assert_eq!(nodes[1].id, "checkpoint:cp-1");
        assert_eq!(nodes[2].id, "a1");
        assert_eq!(nodes[0].checkpoint_id, None);
        assert_eq!(nodes[2].checkpoint_id, Some("cp-1".to_string()));
    }

    #[test]
    fn iteration_details_back_an_empty_trunk() {
        let iterations = vec![
            serde_json::json!({"iteration": 1, "response_content": "done", "timestamp": 100}),
            serde_json::json!({"iteration": 1, "tool_calls": [{"name": "read", "success": true}], "timestamp": 200}),
        ];
        let (nodes, _) = assemble_tree(&[], &[], &iterations);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].kind, TreeNodeKind::Assistant);
        assert_eq!(nodes[1].kind, TreeNodeKind::Tool);
        assert_eq!(nodes[1].tool_name.as_deref(), Some("read"));
    }

    #[test]
    fn filters_hide_system_and_tool_rows() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        assert_eq!(tree.len(), 4);
        tree.set_filter(TreeFilter::NoTools);
        // No tool rows in the sample; count stays.
        assert_eq!(tree.len(), 4);
        tree.set_filter(TreeFilter::All);
        assert_eq!(tree.len(), 4);
    }

    #[test]
    fn search_matches_all_tokens_case_insensitively() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        tree.search_push('R');
        tree.search_push('E');
        tree.search_push('A');
        tree.search_push('D');
        // Substring match: "read the file" and "reading now"; the
        // hidden tail falls back to the nearest visible row.
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.selected_node().map(|n| n.id.as_str()), Some("a1"));
        tree.search_backspace();
        tree.search_backspace();
        tree.search_backspace();
        tree.search_backspace();
        assert_eq!(tree.len(), 4);
        // Multi-token AND: both tokens must match; only one row qualifies.
        for c in "read file".chars() {
            tree.search_push(c);
        }
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.selected_node().map(|n| n.id.as_str()), Some("u1"));
    }

    #[test]
    fn fold_hides_section_rows_and_keeps_selection() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        // Rows: u1, checkpoint, a1, u2; land starts at the tail (u2).
        // Walk back twice onto the checkpoint header.
        tree.move_prev();
        tree.move_prev();
        assert_eq!(
            tree.selected_node().map(|n| n.kind),
            Some(TreeNodeKind::Checkpoint)
        );
        tree.toggle_fold();
        // Section rows (a1, u2) hide; header stays.
        assert_eq!(tree.len(), 2);
        // Selection stays on the still-visible header.
        assert_eq!(
            tree.selected_node().map(|n| n.kind),
            Some(TreeNodeKind::Checkpoint)
        );
        tree.toggle_fold();
        assert_eq!(tree.len(), 4);
    }

    #[test]
    fn selection_falls_to_nearest_following_row() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        assert_eq!(tree.selected_node().map(|n| n.id.as_str()), Some("u2"));
        for c in "file".chars() {
            tree.search_push(c);
        }
        // u2 hidden and only u1 matches; selection falls back to row 0.
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.selected(), 0);
    }

    #[test]
    fn movement_wraps_and_pages_clamp() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        // Land starts at the tail (index 3).
        assert_eq!(tree.selected(), 3);
        tree.move_next();
        assert_eq!(tree.selected(), 0);
        tree.move_prev();
        assert_eq!(tree.selected(), 3);
        tree.jump_to_first();
        tree.page_by(100);
        assert_eq!(tree.selected(), 3);
        tree.page_by(-100);
        assert_eq!(tree.selected(), 0);
    }

    #[test]
    fn rows_mark_fold_state_and_live_tail() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        let rows = tree.rows();
        assert!(rows[1].starts_with("▾ ⬢"), "expanded header: {}", rows[1]);
        assert!(rows[3].ends_with(" ●"), "live tail: {}", rows[3]);
        tree.move_prev();
        tree.move_prev();
        tree.toggle_fold();
        let rows = tree.rows();
        assert!(rows[1].starts_with("▸ ⬢"), "folded header: {}", rows[1]);
        tree.toggle_fold();
        let rows = tree.rows();
        assert!(rows[1].starts_with("▾ ⬢"), "expanded header: {}", rows[1]);
    }

    #[test]
    fn status_line_reports_position_filter_and_search() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        // Land starts at the tail.
        assert!(tree.status_line().starts_with("(4/4) · [default]"));
        tree.search_push('e');
        assert!(tree.status_line().contains("search: e"));
    }

    #[test]
    fn node_cap_keeps_the_newest_messages() {
        let messages: Vec<Message> = (0..SESSION_TREE_NODE_CAP + 10)
            .map(|i| user_message(&format!("u{i}"), "hi", i as i64))
            .collect();
        let (nodes, truncated) = assemble_tree(&messages, &[], &[]);
        assert!(truncated);
        assert_eq!(nodes.len(), SESSION_TREE_NODE_CAP);
        assert_eq!(nodes.first().map(|n| n.id.as_str()), Some("u10"));
    }

    #[test]
    fn checkpoint_points_flatten_both_response_shapes() {
        let loop_view = serde_json::json!({"items": [
            {"id": "cp-1", "timestamp": 100},
            {"id": "cp-2", "timestamp": 200},
        ]});
        assert_eq!(checkpoint_points(&loop_view).len(), 2);
        let chain_view = serde_json::json!({"items": [
            {"checkpoints": [{"id": "cp-3", "timestamp": 300}]},
        ]});
        let points = checkpoint_points(&chain_view);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].id, "cp-3");
    }

    #[test]
    fn bookmark_toggle_filter_and_rows() {
        let mut tree = open_tree();
        land_sample(&mut tree);
        assert_eq!(tree.len(), 4);
        // Mark the second row (checkpoint header).
        tree.move_prev();
        tree.move_prev();
        let toggled = tree.toggle_bookmark_selected();
        assert!(toggled.is_some_and(|(_, _, marked)| marked));
        assert!(tree.rows()[1].contains("★"));
        assert!(tree.status_line().contains("marked: 1"));
        // The marked filter shows only the bookmarked row.
        tree.set_filter(TreeFilter::Bookmarked);
        assert_eq!(tree.len(), 1);
        // Unmarking while filtered empties the view.
        let toggled = tree.toggle_bookmark_selected();
        assert!(toggled.is_some_and(|(_, _, marked)| !marked));
        assert!(tree.is_empty());
        // Restoring the default filter shows the trunk again.
        tree.set_filter(TreeFilter::Default);
        assert_eq!(tree.len(), 4);
    }

    #[test]
    fn bookmark_filter_cycle_includes_marked() {
        assert_eq!(TreeFilter::All.cycle(), TreeFilter::Bookmarked);
        assert_eq!(TreeFilter::Bookmarked.cycle(), TreeFilter::Default);
        assert_eq!(TreeFilter::Bookmarked.label(), "marked");
    }

    #[test]
    fn tool_role_messages_become_tool_rows() {
        let mut message = user_message("t1", "result body", 1000);
        message.role = MessageRole::Tool;
        message.tool_name = Some("read".to_string());
        let (nodes, _) = assemble_tree(&[message], &[], &[]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, TreeNodeKind::Tool);
        assert_eq!(nodes[0].tool_name.as_deref(), Some("read"));
    }
}
