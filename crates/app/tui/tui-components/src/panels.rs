//! Selection panels for the inline footer prompt view: the `/` command
//! palette plus the model / skill / queued-prompt panels.
//!
//! Every panel wraps a [`SelectList`] (grouped scrolling list) and stays
//! pure data: navigation delegates to the list, filtering goes through
//! [`SelectList::set_filter`] (case-insensitive substring) and rendering
//! produces ratatui lines for a
//! caller-provided width / window height.
//!
//! The wf-tui footer event loop owns the interaction policy: it routes
//! keymap actions (`MovePrev`/`MoveNext`/`Select`/`Delete`/`Edit`/`Clear`)
//! into the panel and interprets the selected item's `data`.
//!
//! This module re-exports from focused sub-modules:
//! - Command palette data types and panel (`CommandId`, `CommandEntry`,
//!   `CommandPalette`)
//! - Model / skill / workflow / mention / queued panels
//! - Shared render helper and [`Renderable`] implementations

pub mod command;
pub mod mention;
pub mod model;
pub mod queued;
pub mod render;
pub mod skill;
pub mod workflow;

pub use command::{CommandEntry, CommandId, CommandPalette};
pub use mention::MentionPanel;
pub use model::ModelPanel;
pub use queued::QueuedPanel;
pub use skill::SkillPanel;
pub use workflow::WorkflowPanel;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::queue::QueuedPrompt;
    use tui_core::keymap::KeyAction;
    use wf_types::llm::LlmFormat;
    use wf_types::llm::LlmProfile;
    use wf_types::skill::SkillMetadata;

    fn profile(id: &str, model: &str) -> LlmProfile {
        LlmProfile {
            id: id.into(),
            name: format!("{id} display"),
            format: LlmFormat::Anthropic,
            provider_id: None,
            model: model.into(),
            api_key: None,
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
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
        let profiles = vec![
            profile("default", "claude-3"),
            profile("fast", "gpt-4o-mini"),
        ];
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

    #[test]
    fn workflow_panel_lists_and_moves() {
        let summaries = vec![
            wf_api::workflow::summary::WorkflowSummary {
                id: "wf-1".to_string(),
                name: "First".to_string(),
                description: Some("desc".to_string()),
                version: None,
                node_count: 3,
                edge_count: 2,
                updated_at: 0,
            },
            wf_api::workflow::summary::WorkflowSummary {
                id: "wf-2".to_string(),
                name: "Second".to_string(),
                description: None,
                version: None,
                node_count: 2,
                edge_count: 1,
                updated_at: 0,
            },
        ];
        let mut panel = WorkflowPanel::new(&summaries);
        assert_eq!(panel.selected_workflow(), Some("wf-1".to_string()));
        panel.handle(KeyAction::MoveNext);
        assert_eq!(panel.selected_workflow(), Some("wf-2".to_string()));
    }
}
