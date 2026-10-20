//! Combined `@` mention panel: candidates grouped by tone (files,
//! directories, skills, workflows, resources, tools, scripts, agents) with a
//! shared filter, a visible/total indicator and a mode switch.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::mention::{mention_tone, MentionTone};
use crate::panels::filter::{apply, PanelFilter};
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;

/// One candidate row shown in the mention panel. `value` is the raw target
/// inserted into the composer as `kind:value` on selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionEntry {
    pub kind: String,
    pub value: String,
    pub description: Option<String>,
}

impl MentionEntry {
    pub fn new(kind: impl Into<String>, value: impl Into<String>) -> Self {
        let kind = kind.into();
        let kind = match kind.as_str() {
            "wf" => "workflow".to_string(),
            "agent_loop" => "agent".to_string(),
            _ => kind,
        };
        Self {
            kind,
            value: value.into(),
            description: None,
        }
    }

    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Tone of this candidate, shared with the web mention palette.
    pub fn tone(&self) -> MentionTone {
        mention_tone(&self.kind, &self.value)
    }

    /// Canonical `@`-less reference inserted on selection.
    pub fn reference(&self) -> String {
        format!("{}:{}", self.kind, self.value)
    }
}

/// Which mention groups the panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionMode {
    All,
    FilesAndDirs,
    Files,
    Directories,
    Resources,
}

impl MentionMode {
    const ORDER: [MentionMode; 5] = [
        MentionMode::All,
        MentionMode::FilesAndDirs,
        MentionMode::Files,
        MentionMode::Directories,
        MentionMode::Resources,
    ];

    /// Label shown in the panel header.
    pub fn label(self) -> &'static str {
        match self {
            MentionMode::All => "all",
            MentionMode::FilesAndDirs => "files+dirs",
            MentionMode::Files => "files",
            MentionMode::Directories => "directories",
            MentionMode::Resources => "resources",
        }
    }

    /// Next mode in the cycle.
    pub fn next(self) -> Self {
        let idx = Self::ORDER
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or(0);
        Self::ORDER[(idx + 1) % Self::ORDER.len()]
    }

    fn shows(self, tone: MentionTone) -> bool {
        match self {
            MentionMode::All => true,
            MentionMode::FilesAndDirs => {
                matches!(tone, MentionTone::File | MentionTone::Directory)
            }
            MentionMode::Files => tone == MentionTone::File,
            MentionMode::Directories => tone == MentionTone::Directory,
            MentionMode::Resources => tone == MentionTone::Resource,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MentionPanel {
    list: SelectList<String>,
    entries: Vec<MentionEntry>,
    filter: PanelFilter,
    mode: MentionMode,
}

impl MentionPanel {
    /// Build the panel from mention candidates. `filter` is an optional
    /// substring filter applied to labels.
    pub fn new(entries: &[MentionEntry], filter: Option<&str>) -> Self {
        let mut panel = Self {
            list: SelectList::default(),
            entries: entries.to_vec(),
            filter: PanelFilter::new(),
            mode: MentionMode::All,
        };
        panel.rebuild();
        if let Some(filter) = filter {
            panel.set_filter(Some(filter));
        }
        panel
    }

    /// Rebuild the grouped list for the current mode and filter.
    fn rebuild(&mut self) {
        let mut groups: Vec<Group<String>> = Vec::new();
        for tone in MentionTone::ORDER {
            if !self.mode.shows(tone) {
                continue;
            }
            let items: Vec<&MentionEntry> = self
                .entries
                .iter()
                .filter(|entry| entry.tone() == tone)
                .collect();
            if items.is_empty() {
                continue;
            }
            let mut group = Group::new(Some(tone.title()));
            for entry in items {
                let label = format!("{} {}", tone.icon(), entry.value);
                let mut item = GroupItem::new(label, entry.reference()).styled(tone.style());
                if let Some(description) = &entry.description {
                    item = item.described(description.clone());
                }
                group = group.item(item);
            }
            groups.push(group);
        }
        if groups.is_empty() {
            groups.push(Group::new(Some("mentions")));
        }
        let list = SelectList::groups(groups).with_group_titles(true);
        self.list = list;
        // Re-apply the filter over the rebuilt groups.
        let text = self.filter.text().to_string();
        apply(&mut self.list, &text);
    }

    /// The candidate reference under the cursor.
    pub fn selected_candidate(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Replace the candidate set, keeping the current filter and mode. Used
    /// when fresh candidates land for the open mention query.
    pub fn set_entries(&mut self, entries: &[MentionEntry]) {
        self.entries = entries.to_vec();
        self.rebuild();
    }

    /// The filter text typed so far.
    pub fn filter_text(&self) -> &str {
        self.filter.text()
    }

    /// Current mode.
    pub fn mode(&self) -> MentionMode {
        self.mode
    }

    /// Advance to the next mode and rebuild the visible groups.
    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.next();
        self.rebuild();
    }

    /// Push a character into the filter.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        apply(&mut self.list, self.filter.text());
    }

    /// Pop the last character from the filter.
    pub fn filter_backspace(&mut self) {
        self.filter.backspace();
        apply(&mut self.list, self.filter.text());
    }

    /// Re-apply a filter (fuzzy substring).
    pub fn set_filter(&mut self, filter: Option<&str>) {
        self.filter.set(filter.unwrap_or_default());
        apply(&mut self.list, self.filter.text());
    }

    /// Apply a keymap action; returns whether it was consumed.
    pub fn handle(&mut self, action: KeyAction) -> bool {
        match action {
            KeyAction::MovePrev => {
                self.list.navigate(NavigateDir::Prev);
                true
            }
            KeyAction::MoveNext => {
                self.list.navigate(NavigateDir::Next);
                true
            }
            KeyAction::CycleMode => {
                self.cycle_mode();
                true
            }
            KeyAction::Select | KeyAction::Submit | KeyAction::Cancel | KeyAction::Back => true,
            _ => false,
        }
    }

    /// Render the panel rows: a mode/position header, then the grouped list.
    pub fn render_lines(
        &self,
        width: u16,
        window_height: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        let header = format!("{} · {}", self.mode.label(), self.list.position_string());
        let mut lines = vec![Line::from(Span::styled(
            header,
            Style::default().add_modifier(Modifier::DIM),
        ))];
        lines.extend(self.list.render_lines(width, window_height.saturating_sub(1)));
        lines
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }

    /// Number of visible candidates.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, value: &str) -> MentionEntry {
        MentionEntry::new(kind, value)
    }

    fn plain(panel: &MentionPanel, width: u16) -> String {
        panel
            .render_lines(width, 16)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn groups_every_tone_with_titles() {
        let entries = vec![
            entry("file", "src/main.rs"),
            entry("file", "src/"),
            entry("skill", "pdf"),
            entry("workflow", "wf-1"),
            entry("resource", "file:///tmp"),
        ];
        let panel = MentionPanel::new(&entries, None);
        let text = plain(&panel, 60);
        assert!(text.contains("FILES"), "{text}");
        assert!(text.contains("DIRECTORIES"), "{text}");
        assert!(text.contains("SKILLS"), "{text}");
        assert!(text.contains("WORKFLOWS"), "{text}");
        assert!(text.contains("RESOURCES"), "{text}");
        assert!(text.contains("src/main.rs"), "{text}");
        assert!(text.contains("src/"), "{text}");
        assert!(text.contains("pdf"), "{text}");
        assert!(text.contains("wf-1"), "{text}");
        assert!(text.contains("file:///tmp"), "{text}");
    }

    #[test]
    fn mode_switch_filters_groups() {
        let entries = vec![
            entry("file", "src/main.rs"),
            entry("file", "src/"),
            entry("resource", "file:///tmp"),
        ];
        let mut panel = MentionPanel::new(&entries, None);
        assert_eq!(panel.mode(), MentionMode::All);
        panel.cycle_mode();
        assert_eq!(panel.mode(), MentionMode::FilesAndDirs);
        assert!(plain(&panel, 60).contains("src/main.rs"));
        panel.cycle_mode();
        assert_eq!(panel.mode(), MentionMode::Files);
        let files = plain(&panel, 60);
        assert!(files.contains("src/main.rs"), "{files}");
        assert!(!files.contains("file:///tmp"), "{files}");
        panel.cycle_mode();
        assert_eq!(panel.mode(), MentionMode::Directories);
        let dirs = plain(&panel, 60);
        assert!(dirs.contains("src/"), "{dirs}");
        assert!(!dirs.contains("src/main.rs"), "{dirs}");
    }

    #[test]
    fn handle_cycles_mode_and_moves() {
        let entries = vec![entry("file", "a.rs"), entry("file", "b.rs")];
        let mut panel = MentionPanel::new(&entries, None);
        assert!(panel.handle(KeyAction::CycleMode));
        assert_eq!(panel.mode(), MentionMode::FilesAndDirs);
        assert!(panel.handle(KeyAction::MoveNext));
        assert_eq!(panel.selected_candidate(), Some("file:b.rs".to_string()));
    }

    #[test]
    fn position_string_reports_visible_total() {
        let entries = vec![entry("skill", "a"), entry("skill", "b")];
        let panel = MentionPanel::new(&entries, None);
        assert_eq!(panel.position_string(), "(1/2)");
    }

    #[test]
    fn empty_panel_still_renders_header() {
        let panel = MentionPanel::new(&[], None);
        let text = plain(&panel, 60);
        assert!(text.contains("(0/0)"), "{text}");
    }
}
