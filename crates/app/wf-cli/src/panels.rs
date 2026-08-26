//! Selection panels for the mini footer prompt view: the `/` command
//! palette plus the model / skill / queued-prompt panels.
//!
//! Every panel wraps a [`SelectList`] (Stage 4 grouped scrolling list) and
//! stays pure data: navigation delegates to the list, filtering goes through
//! [`SelectList::set_filter`] (case-insensitive substring, the P0 semantics
//! from the stage 4 plan) and rendering produces ratatui lines for a
//! caller-provided width / window height.
//!
//! The mini event loop owns the interaction policy: it routes keymap
//! actions (`MovePrev`/`MoveNext`/`Select`/`Delete`/`Edit`/`Clear`) into the
//! panel and interprets the selected item's `data`.

use crate::keymap::KeyAction;
use crate::queue::QueuedPrompt;
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use wf_types::llm::LlmProfile;
use wf_types::skill::SkillMetadata;

/// Identifies a command palette entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    /// `/new` — clear the conversation and start a fresh session.
    New,
    /// `/model` — open the model panel.
    Model,
    /// `/skills` — open the skill panel.
    Skill,
    /// `/queued` — open the queued prompt panel.
    Queued,
    /// `/editor` — edit the composer draft in `$EDITOR`.
    Editor,
    /// `/quit` — leave the mini session.
    Quit,
    /// `/help` — show the keymap and command help.
    Help,
}

/// One command palette row.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub id: CommandId,
    pub label: &'static str,
    pub description: &'static str,
}

/// The `/` command palette.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    list: SelectList<CommandId>,
    entries: Vec<CommandEntry>,
    /// Filter text typed while the palette is open.
    filter: String,
}

impl CommandPalette {
    /// Built-in command set.
    pub fn new() -> Self {
        let entries = Self::builtin_entries();
        let list = Self::build_list(&entries, "");
        Self {
            list,
            entries,
            filter: String::new(),
        }
    }

    fn builtin_entries() -> Vec<CommandEntry> {
        vec![
            CommandEntry {
                id: CommandId::New,
                label: "/new",
                description: "start a new session (clears the conversation)",
            },
            CommandEntry {
                id: CommandId::Model,
                label: "/model",
                description: "pick the model profile for the next turns",
            },
            CommandEntry {
                id: CommandId::Skill,
                label: "/skills",
                description: "list and run a skill",
            },
            CommandEntry {
                id: CommandId::Queued,
                label: "/queued",
                description: "manage queued prompts",
            },
            CommandEntry {
                id: CommandId::Editor,
                label: "/editor",
                description: "edit the draft in $EDITOR",
            },
            CommandEntry {
                id: CommandId::Quit,
                label: "/quit",
                description: "exit the mini session",
            },
            CommandEntry {
                id: CommandId::Help,
                label: "/help",
                description: "show the mini keymap and command help",
            },
        ]
    }

    fn build_list(entries: &[CommandEntry], filter: &str) -> SelectList<CommandId> {
        let group = Group::new(Some("commands"));
        let mut group = group;
        for entry in entries {
            group = group.item(GroupItem::new(entry.label, entry.id).described(entry.description));
        }
        let mut list = SelectList::groups(vec![group]);
        list.set_filter(if filter.is_empty() {
            None
        } else {
            Some(filter)
        });
        list
    }

    /// The command highlighted by the cursor.
    pub fn selected_command(&self) -> Option<CommandId> {
        self.list.selected().map(|item| item.data)
    }

    /// Apply a keymap action; returns `true` when the action was consumed.
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
            KeyAction::HistoryPrev => {
                self.list.navigate(NavigateDir::Prev);
                true
            }
            KeyAction::HistoryNext => {
                self.list.navigate(NavigateDir::Next);
                true
            }
            KeyAction::Clear => {
                self.filter.clear();
                self.list.set_filter(None);
                true
            }
            _ => false,
        }
    }

    /// Append a character to the filter.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.list.set_filter(Some(self.filter.as_str()));
    }

    /// Remove the last grapheme from the filter.
    pub fn filter_backspace(&mut self) {
        let trimmed: String = self
            .filter
            .graphemes(true)
            .rev()
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        self.filter = trimmed;
        self.list.set_filter(if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.as_str())
        });
    }

    /// The filter text typed so far.
    pub fn filter_text(&self) -> &str {
        &self.filter
    }

    /// Look a command up by its typed label (`"/model"`, leading `/`
    /// optional). Used by direct `/command` submits.
    pub fn find(&self, typed: &str) -> Option<CommandId> {
        let needle = typed.trim();
        let needle = needle.strip_prefix('/').unwrap_or(needle);
        self.entries
            .iter()
            .find(|e| e.label.strip_prefix('/').unwrap_or(e.label) == needle)
            .map(|e| e.id)
    }

    /// Number of visible (filter-passing) commands.
    pub fn visible_len(&self) -> usize {
        self.list.len()
    }

    /// Render the palette rows for the given width / window height.
    pub fn render_lines(&self, width: u16, window_height: u16) -> Vec<ratatui::text::Line<'static>> {
        self.list.render_lines(width, window_height)
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }
}

use unicode_segmentation::UnicodeSegmentation;

/// The model profile panel.
#[derive(Debug, Clone)]
pub struct ModelPanel {
    list: SelectList<String>,
    /// Currently active profile id (marked in the list).
    current: Option<String>,
}

impl ModelPanel {
    /// Build the panel from the gateway profile list.
    pub fn new(profiles: &[LlmProfile], current: Option<&str>) -> Self {
        let mut group = Group::new(Some("model profiles"));
        for profile in profiles {
            let label = if Some(profile.id.as_str()) == current {
                format!("{} · {} (active)", profile.id, profile.model)
            } else {
                format!("{} · {}", profile.id, profile.model)
            };
            group = group.item(
                GroupItem::new(label, profile.id.clone())
                    .described(profile.name.clone()),
            );
        }
        // SelectList tracks position over filtered candidates; with no
        // filter every item is a candidate, so the flat index (position in
        // `profiles`) equals the candidate index.
        let cursor = current.and_then(|id| profiles.iter().position(|p| p.id == id));
        let list = SelectList::groups(vec![group]);
        let mut panel = Self {
            list,
            current: current.map(str::to_string),
        };
        if let Some(cursor) = cursor {
            panel.list.move_to(cursor);
        }
        panel
    }

    /// The profile id under the cursor.
    pub fn selected_model(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Whether the cursor sits on the active profile.
    pub fn on_current(&self) -> bool {
        self.selected_model()
            .is_some_and(|id| Some(&id) == self.current.as_ref())
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
            _ => false,
        }
    }

    /// Render the panel rows.
    pub fn render_lines(
        &self,
        width: u16,
        window_height: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        self.list.render_lines(width, window_height)
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }
}

/// The skill panel.
#[derive(Debug, Clone)]
pub struct SkillPanel {
    list: SelectList<String>,
}

impl SkillPanel {
    /// Build the panel from the loader's skill list.
    pub fn new(skills: &[SkillMetadata]) -> Self {
        let mut group = Group::new(Some("skills"));
        for skill in skills {
            group = group.item(
                GroupItem::new(skill.name.clone(), skill.name.clone())
                    .described(skill.description.clone()),
            );
        }
        Self {
            list: SelectList::groups(vec![group]),
        }
    }

    /// The skill name under the cursor.
    pub fn selected_skill(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
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
            _ => false,
        }
    }

    /// Render the panel rows.
    pub fn render_lines(
        &self,
        width: u16,
        window_height: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        self.list.render_lines(width, window_height)
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }
}

/// The queued prompt panel (edit / delete entries of a [`crate::queue::PromptQueue`]).
#[derive(Debug, Clone)]
pub struct QueuedPanel {
    list: SelectList<u64>,
}

impl QueuedPanel {
    /// Rebuild the panel from the current queue contents.
    pub fn new(items: &[QueuedPrompt]) -> Self {
        let mut group = Group::new(Some("queued prompts"));
        for prompt in items {
            group = group.item(GroupItem::new(prompt.text.clone(), prompt.id));
        }
        Self {
            list: SelectList::groups(vec![group]),
        }
    }

    /// The queued prompt id under the cursor.
    pub fn selected_id(&self) -> Option<u64> {
        self.list.selected().map(|item| item.data)
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
            _ => false,
        }
    }

    /// Render the panel rows.
    pub fn render_lines(
        &self,
        width: u16,
        window_height: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        self.list.render_lines(width, window_height)
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::llm::LlmProvider;

    fn profile(id: &str, model: &str) -> LlmProfile {
        LlmProfile {
            id: id.into(),
            name: format!("{id} display"),
            provider: LlmProvider::Anthropic,
            model: model.into(),
            api_key: None,
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }
    }

    fn skill(name: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.into(),
            description: format!("{name} description"),
            when_to_use: None,
            version: None,
            license: None,
            allowed_tools: None,
            metadata: None,
        }
    }

    #[test]
    fn palette_lists_builtin_commands_and_navigates() {
        let mut palette = CommandPalette::new();
        assert!(palette.visible_len() >= 7);
        assert_eq!(palette.selected_command(), Some(CommandId::New));
        palette.handle(KeyAction::MoveNext);
        assert_eq!(palette.selected_command(), Some(CommandId::Model));
        palette.handle(KeyAction::MovePrev);
        assert_eq!(palette.selected_command(), Some(CommandId::New));
    }

    #[test]
    fn palette_finds_commands_by_typed_label() {
        let palette = CommandPalette::new();
        assert_eq!(palette.find("/model"), Some(CommandId::Model));
        assert_eq!(palette.find("quit"), Some(CommandId::Quit));
        assert_eq!(palette.find("help"), Some(CommandId::Help));
        assert_eq!(palette.find("nope"), None);
    }

    #[test]
    fn palette_filter_narrows_and_clears() {
        let mut palette = CommandPalette::new();
        palette.filter_push('m');
        palette.filter_push('o');
        assert!(palette.visible_len() < 7, "filter narrows the list");
        assert_eq!(palette.selected_command(), Some(CommandId::Model));
        palette.handle(KeyAction::Clear);
        assert!(palette.visible_len() >= 7);
        // Backspace on an empty filter is a no-op.
        palette.filter_backspace();
        assert!(palette.visible_len() >= 7);
    }

    #[test]
    fn model_panel_marks_and_positions_the_current_profile() {
        let profiles = vec![profile("default", "claude-3"), profile("fast", "gpt-4o-mini")];
        let panel = ModelPanel::new(&profiles, Some("fast"));
        assert_eq!(panel.selected_model(), Some("fast".to_string()));
        assert!(panel.on_current());
        let mut panel = panel;
        panel.handle(KeyAction::MovePrev);
        assert_eq!(panel.selected_model(), Some("default".to_string()));
        assert!(!panel.on_current());
    }

    #[test]
    fn skill_panel_lists_skills_and_moves() {
        let skills = vec![skill("pdf"), skill("xlsx")];
        let mut panel = SkillPanel::new(&skills);
        assert_eq!(panel.selected_skill(), Some("pdf".to_string()));
        panel.handle(KeyAction::MoveNext);
        assert_eq!(panel.selected_skill(), Some("xlsx".to_string()));
    }

    #[test]
    fn queued_panel_exposes_the_selected_id() {
        let items = vec![
            QueuedPrompt {
                id: 1,
                text: "first".into(),
            },
            QueuedPrompt {
                id: 2,
                text: "second".into(),
            },
        ];
        let mut panel = QueuedPanel::new(&items);
        assert_eq!(panel.selected_id(), Some(1));
        panel.handle(KeyAction::MoveNext);
        assert_eq!(panel.selected_id(), Some(2));
        assert!(panel.position_string().starts_with("(2/2)"));
    }
}
