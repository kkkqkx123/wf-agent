//! Selection panels for the inline footer prompt view: the `/` command
//! palette plus the model / skill / queued-prompt panels.
//!
//! Every panel wraps a [`crate::select::SelectList`] (grouped scrolling list) and stays
//! pure data: navigation delegates to the list, filtering goes through
//! [`crate::select::SelectList::set_filter`] (case-insensitive substring) and rendering
//! produces ratatui lines for a
//! caller-provided width / window height.
//!
//! The wf-tui footer event loop owns the interaction policy: it routes
//! keymap actions (`MovePrev`/`MoveNext`/`Select`/`Delete`/`Edit`/`Clear`)
//! into the panel and interprets the selected item's `data`.
//!
//! This module re-exports from focused sub-modules:
//! - The command palette (`CommandPalette`) over the command registry
//! - Model / skill / workflow / mention / queued panels
//! - Shared render helper and [`tui_core::renderable::Renderable`] implementations

pub mod auth;
pub mod command;
pub mod filter;
pub mod mention;
pub mod model;
pub mod queued;
pub mod render;
pub mod skill;
pub mod workflow;

pub use auth::{AuthPanel, AuthPromptOption, AuthSessionView, AuthSurfaceRow};
pub use command::CommandPalette;
pub use filter::PanelFilter;
pub use mention::{MentionEntry, MentionMode, MentionPanel};
pub use model::ModelPanel;
pub use queued::QueuedPanel;
pub use skill::SkillPanel;
pub use workflow::{WorkflowPanel, WorkflowSummaryRow};

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
            max_output_tokens: None,
            proxy: None,
            no_proxy: None,
            circuit_breaker: None,
            prompt_caching: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
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
        assert_eq!(
            palette.selected_command().map(|command| command.id),
            Some("app.quit")
        );
        palette.handle(KeyAction::MoveNext);
        assert_eq!(
            palette.selected_command().map(|command| command.id),
            Some("app.help")
        );
        palette.handle(KeyAction::MovePrev);
        assert_eq!(
            palette.selected_command().map(|command| command.id),
            Some("app.quit")
        );
    }

    #[test]
    fn palette_finds_commands_by_typed_label() {
        let palette = CommandPalette::new();
        assert_eq!(
            palette.find("/model").map(|command| command.id),
            Some("session.model")
        );
        assert_eq!(
            palette.find("quit").map(|command| command.id),
            Some("app.quit")
        );
        assert_eq!(
            palette.find("help").map(|command| command.id),
            Some("app.help")
        );
        assert_eq!(
            palette.find("/copy").map(|command| command.id),
            Some("session.copy")
        );
        assert_eq!(
            palette.find("copy").map(|command| command.id),
            Some("session.copy")
        );
        assert!(palette.find("nope").is_none());
    }

    #[test]
    fn palette_filter_narrows_and_clears() {
        let mut palette = CommandPalette::new();
        palette.filter_push('m');
        palette.filter_push('o');
        assert!(palette.visible_len() < 7, "filter narrows the list");
        assert_eq!(
            palette.selected_command().map(|command| command.id),
            Some("session.model")
        );
        palette.handle(KeyAction::Clear);
        assert!(palette.visible_len() >= 7);
        // Backspace on an empty filter is a no-op.
        palette.filter_backspace();
        assert!(palette.visible_len() >= 7);
    }

    #[test]
    fn palette_keeps_the_command_argument_apart() {
        let mut palette = CommandPalette::with_filter("/");
        for c in "copy all".chars() {
            palette.filter_push(c);
        }
        assert_eq!(palette.argument(), "all");
        assert_eq!(
            palette.selected_command().map(|command| command.id),
            Some("session.copy")
        );
        assert_eq!(palette.visible_len(), 1);
    }

    #[test]
    fn model_panel_filters_its_candidates() {
        let profiles = vec![
            profile("default", "claude-3"),
            profile("fast", "gpt-4o-mini"),
        ];
        let mut panel = ModelPanel::new(&profiles, None);
        panel.filter_push('f');
        assert_eq!(panel.selected_model(), Some("fast".to_string()));
        panel.filter_backspace();
        assert_eq!(panel.filter_text(), "");
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
            super::workflow::WorkflowSummaryRow {
                id: "wf-1".to_string(),
                name: "First".to_string(),
                description: Some("desc".to_string()),
                node_count: 3,
                edge_count: 2,
            },
            super::workflow::WorkflowSummaryRow {
                id: "wf-2".to_string(),
                name: "Second".to_string(),
                description: None,
                node_count: 2,
                edge_count: 1,
            },
        ];
        let mut panel = WorkflowPanel::new(&summaries);
        assert_eq!(panel.selected_workflow(), Some("wf-1".to_string()));
        panel.handle(KeyAction::MoveNext);
        assert_eq!(panel.selected_workflow(), Some("wf-2".to_string()));
    }
}
