//! `SelectList`: the single navigation component for every list screen.
//! It is a grouped, single-cursor list with a pure
//! viewport computation and a theme-agnostic renderer.
//!
//! The list is domain-free: items carry generic `data` (often `()` for pure
//! tests) and only a `label` + optional `description` need a render shape.
//! Per the component red lines it never wraps the row into a `Block`/border
//! and renders a single truncated row per item.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Navigation direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigateDir {
    Prev,
    Next,
}

/// Basic category of a [`Group`].
#[derive(Debug, Clone, Default)]
pub struct Group<T = ()> {
    pub title: Option<String>,
    pub items: Vec<GroupItem<T>>,
}

impl<T> Group<T> {
    pub fn new(title: Option<&str>) -> Self {
        Self {
            title: title.map(str::to_owned),
            items: Vec::new(),
        }
    }

    pub fn item(mut self, item: GroupItem<T>) -> Self {
        self.items.push(item);
        self
    }
}

/// One row in a group.
#[derive(Debug, Clone, Default)]
pub struct GroupItem<T = ()> {
    pub label: String,
    pub description: Option<String>,
    pub data: T,
}

impl<T> GroupItem<T> {
    pub fn new(label: impl Into<String>, data: T) -> Self {
        Self {
            label: label.into(),
            description: None,
            data,
        }
    }

    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A grouped, scrollable, single-cursor list.
///
/// Selection is tracked as a *position in the filtered candidate list*, so
/// filtering ([`Slanew` filter]) and navigation share one code path.
#[derive(Debug, Clone)]
pub struct SelectList<T = ()> {
    groups: Vec<Group<T>>,
    filter: Option<String>,
    wrap: bool,
    select_pos: usize,
}

impl<T> Default for SelectList<T> {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            filter: None,
            wrap: true,
            select_pos: 0,
        }
    }
}

impl<T> SelectList<T> {
    pub fn groups(groups: Vec<Group<T>>) -> Self {
        Self {
            groups,
            ..Self::default()
        }
    }

    pub fn group(mut self, group: Group<T>) -> Self {
        self.groups.push(group);
        self
    }

    /// Move the cursor one step; `wrap` is honoured when set and `false`
    /// clamps at the ends. Returns whether the cursor actually moved.
    pub fn navigate(&mut self, dir: NavigateDir) -> bool {
        let n = self.len();
        if n == 0 {
            return false;
        }
        let next = match dir {
            NavigateDir::Prev => {
                if self.select_pos > 0 {
                    self.select_pos - 1
                } else if self.wrap {
                    n - 1
                } else {
                    self.select_pos
                }
            }
            NavigateDir::Next => {
                if self.select_pos + 1 < n {
                    self.select_pos + 1
                } else if self.wrap {
                    0
                } else {
                    self.select_pos
                }
            }
        };
        let changed = next != self.select_pos;
        self.select_pos = next;
        changed
    }

    pub fn move_to(&mut self, pos: usize) -> bool {
        let n = self.len();
        if n == 0 {
            self.select_pos = 0;
            return false;
        }
        let clamped = pos.min(n - 1);
        let changed = clamped != self.select_pos;
        self.select_pos = clamped;
        changed
    }

    /// Number of currently-visible (filter-passing) items.
    pub fn len(&self) -> usize {
        self.candidates().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Filter by case-insensitive substring; `None` (or empty) resets it.
    pub fn set_filter(&mut self, needle: Option<&str>) {
        self.filter = needle.map(str::to_owned);
        let n = self.len();
        if !self.is_empty() && self.select_pos >= n {
            self.select_pos = n - 1;
        }
        if n == 0 {
            self.select_pos = 0;
        }
    }

    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    /// The currently selected item, if any.
    pub fn selected(&self) -> Option<&GroupItem<T>> {
        let cands = self.candidates();
        let pos = self.select_pos.min(cands.len().saturating_sub(1));
        self.item_at(cands[pos])
    }

    /// Position in the candidate list (1-based) / total — the `(N/M)`
    /// scroll indicator text.
    pub fn position_string(&self) -> String {
        let n = self.len();
        if n == 0 {
            return "(0/0)".into();
        }
        format!("({}/{})", self.select_pos + 1, n)
    }

    /// Selected-item default highlight style (theme-agnostic reversed).
    pub fn selected_style() -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }

    /// Flat indices (into all items) that pass the filter.
    fn candidates(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut flat = 0usize;
        for g in &self.groups {
            for item in &g.items {
                if self.matches(item) {
                    out.push(flat);
                }
                flat += 1;
            }
        }
        out
    }

    fn matches(&self, item: &GroupItem<T>) -> bool {
        match self.filter.as_deref() {
            None | Some("") => true,
            Some(needle) => item.label.to_lowercase().contains(&needle.to_lowercase()),
        }
    }

    /// Resolve a global flat item index back to a group item.
    fn item_at(&self, flat: usize) -> Option<&GroupItem<T>> {
        let mut i = 0usize;
        for g in &self.groups {
            for item in &g.items {
                if i == flat {
                    return Some(item);
                }
                i += 1;
            }
        }
        None
    }
}

/// Visible window over the candidate list that keeps the selection in view.
fn view_window(select_pos: usize, height: usize, total: usize) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }
    let max_top = total.saturating_sub(height);
    let top = select_pos
        .saturating_sub(height.saturating_sub(1))
        .min(max_top);
    let bottom = (top + height).min(total);
    (top, bottom)
}

/// Join and truncate a row to `width` columns (grapheme-aware).
fn clamp_to_width(s: &str, width: usize) -> String {
    let mut take_w = 0usize;
    let mut out = String::new();
    for g in s.graphemes(true) {
        let gw = g.width();
        if take_w + gw > width {
            break;
        }
        take_w += gw;
        out.push_str(g);
    }
    out
}

impl<T> SelectList<T> {
    /// Render the visible candidate rows as ratatui lines. `width` governs
    /// whether the two-column label/description layout is used; the row is
    /// always a single truncated line.
    pub fn render_lines(&self, width: u16, window_height: u16) -> Vec<Line<'static>> {
        let cands = self.candidates();
        let n = cands.len();
        if n == 0 {
            return vec![Line::from("")];
        }
        let height = usize::from(window_height.max(1));
        let (top, bottom) = view_window(self.select_pos, height, n);
        let w = usize::from(width.max(1));
        let show_desc = w > 40;

        let mut out = Vec::new();
        for (pos, &flat) in cands.iter().enumerate().take(bottom).skip(top) {
            let item = self.item_at(flat);
            let selected = pos == self.select_pos;
            let marker = if selected { "→" } else { " " };

            let mut spans: Vec<Span<'static>> = Vec::new();
            spans.push(Span::styled(
                marker,
                if selected {
                    Self::selected_style()
                } else {
                    Style::default()
                },
            ));
            let mut label = item.map(|i| i.label.clone()).unwrap_or_default();
            if show_desc {
                if let Some(desc) = item.and_then(|i| i.description.as_ref()) {
                    label = format!("{label} - {desc}");
                }
            }
            let label = clamp_to_width(&label, w.saturating_sub(2));
            spans.push(Span::styled(
                label,
                if selected {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ));
            out.push(Line::from(spans));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SelectList<u32> {
        SelectList::groups(vec![
            Group::new(Some("Workflows"))
                .item(GroupItem::new("wf-a", 1))
                .item(GroupItem::new("wf-b", 2)),
            Group::new(Some("Executions"))
                .item(GroupItem::new("exec-1", 3).described("running"))
                .item(GroupItem::new("exec-2", 4).described("idle"))
                .item(GroupItem::new("exec-3", 5)),
        ])
    }

    #[test]
    fn navigation_wraps_by_default() {
        let mut l = sample();
        assert_eq!(l.len(), 5);
        assert_eq!(l.selected().map(|i| i.data), Some(1));
        // Next four moves: 2,3,4,5 then wrap to 1.
        for expected in [2, 3, 4, 5, 1] {
            assert!(l.navigate(NavigateDir::Next));
            assert_eq!(l.selected().map(|i| i.data), Some(expected));
        }
        // Backwards wraps too.
        assert!(l.navigate(NavigateDir::Prev));
        assert_eq!(l.selected().map(|i| i.data), Some(5));
    }

    #[test]
    fn navigation_clamps_when_not_wrapping() {
        let mut l = sample();
        l.wrap = false;
        for _ in 0..10 {
            l.navigate(NavigateDir::Next);
        }
        assert_eq!(l.selected().map(|i| i.data), Some(5));
        for _ in 0..10 {
            l.navigate(NavigateDir::Prev);
        }
        assert_eq!(l.selected().map(|i| i.data), Some(1));
    }

    #[test]
    fn filter_restricts_candidates() {
        let mut l = sample();
        l.set_filter(Some("exec"));
        assert_eq!(l.len(), 3);
        assert_eq!(l.selected().map(|i| i.data), Some(3));
        l.navigate(NavigateDir::Next);
        assert_eq!(l.selected().map(|i| i.data), Some(4));
        l.set_filter(Some("wf-b"));
        assert_eq!(l.len(), 1);
        assert_eq!(l.selected().map(|i| i.data), Some(2));
        l.set_filter(None);
        assert_eq!(l.len(), 5);
    }

    #[test]
    fn pos_string_reports_cursor() {
        let mut l = sample();
        assert_eq!(l.position_string(), "(1/5)");
        l.navigate(NavigateDir::Next);
        l.navigate(NavigateDir::Next);
        assert_eq!(l.position_string(), "(3/5)");
        assert_eq!(SelectList::<u32>::default().position_string(), "(0/0)");
    }

    #[test]
    fn render_lines_marks_selection_and_shows_group_window() {
        let mut l = sample();
        l.move_to(2);
        let lines = l.render_lines(60, 5);
        assert_eq!(lines.len(), 5);
        let second_row_plain: String = lines[2].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            second_row_plain.contains("→"),
            "selected row marked: {second_row_plain}"
        );
        assert!(
            second_row_plain.contains("exec-1"),
            "selected item: {second_row_plain}"
        );
    }

    #[test]
    fn render_lines_scrolls_to_keep_selection_visible() {
        let mut l = sample();
        l.move_to(4); // last item
        let lines = l.render_lines(60, 2);
        assert_eq!(lines.len(), 2, "window height 2");
        let last_row: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            last_row.contains("→"),
            "last item visible & selected: {last_row}"
        );
    }

    #[test]
    fn render_lines_shapes_are_stable() {
        // In-memory golden values (no file writer in lib tests). The full
        // rendered files are (re)generated by the `component_output` example
        // into `crates/wf-cli/outputs/`.
        let mut l = sample();
        l.move_to(3);
        let rend = |width: u16| {
            l.render_lines(width, 5)
                .iter()
                .map(|ln| {
                    ln.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            rend(80),
            " wf-a\n wf-b\n exec-1 - running\n→exec-2 - idle\n exec-3"
        );
        assert_eq!(rend(20), " wf-a\n wf-b\n exec-1\n→exec-2\n exec-3");
    }
}
