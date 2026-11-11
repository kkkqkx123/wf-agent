//! Session tree view state: selection, filter, search, fold and bookmarks.

use std::collections::HashSet;

use super::{AssembledTree, RestoreScope, TreeFilter, TreeNode, TreeNodeKind};

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
