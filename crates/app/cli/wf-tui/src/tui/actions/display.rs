//! Display toggles: mouse, scrollbar, sidebar and tool output.

use crate::modal::{ConfirmModal, ModalResult, ModelPicker};
use crate::overlay::Feedback;
use crate::screens::{short_id, ApprovalRow, CheckpointRow, ConflictRow, ScreenData, ScreenKind};
use wf_client::WfClient;

use super::conflict::step_option;
use super::*;
use crate::tui::data_cache::DataResult;
use crate::tui::TuiApp;

impl TuiApp {
    /// Open the full-text viewer for the selected execution subview.
    /// Overview keeps its replay jump; other views show the cached detail
    /// lines and refetch when they are missing.
    pub(crate) fn open_selected_exec_viewer(&mut self) {
        if self.app.screen.exec_view == crate::screens::ExecView::Overview {
            return;
        }
        let Some((key, _)) = self.current_detail_key() else {
            return;
        };
        let cached = self
            .app
            .screen
            .detail_for(&key)
            .map(|c| (c.title.clone(), c.lines.join("\n")));
        if let Some((title, text)) = cached {
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(title, &text)));
        } else {
            self.app.notice.set("Loading detail...".to_string());
            self.request_detail_for_current();
        }
    }

    /// Open the full-text viewer for the selected workflow subview.
    /// Summary renders from the cached row; other views use fetched lines.
    pub(crate) fn open_selected_workflow_viewer(&mut self) {
        if self.app.screen.workflow_view == crate::screens::WorkflowView::Summary {
            let rows = match self.display_data() {
                ScreenData::Workflow(rows) => rows,
                _ => return,
            };
            let selected = self.app.screen.navigation.selected();
            let Some(row) = rows.get(selected.min(rows.len().saturating_sub(1))) else {
                return;
            };
            let text = format!(
                "id: {}\nname: {}\ndescription: {}\nnodes: {}",
                row.id,
                row.name,
                row.description.as_deref().unwrap_or("-"),
                row.node_count
            );
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(
                    format!("Workflow {}", row.name),
                    &text,
                )));
            return;
        }
        let Some((key, _)) = self.current_detail_key() else {
            return;
        };
        let cached = self
            .app
            .screen
            .detail_for(&key)
            .map(|c| (c.title.clone(), c.lines.join("\n")));
        if let Some((title, text)) = cached {
            self.modals
                .push(Box::new(crate::file_viewer::FileViewer::new(title, &text)));
        } else {
            self.app.notice.set("Loading detail...".to_string());
            self.request_detail_for_current();
        }
    }

    /// Prompt for a client-side substring filter over the current list.
    /// Clearing goes through the `X` key: the prompt never submits empty.
    pub(crate) fn prompt_list_filter(&mut self) {
        let rx = self
            .modals
            .push_with_result(Box::new(crate::prompt_modal::PromptModal::new(
                "Filter list",
                "Show rows containing",
            )));
        let tx = self.feedback_tx.clone();
        self.tasks.push(tokio::spawn(async move {
            if let Ok(ModalResult::Value(text)) = rx.await {
                let _ = tx.send(Feedback::Filter(text.trim().to_string()));
            }
        }));
    }

    /// Apply `apply` to the local config file and persist it. Runtime
    /// policies resolve at startup, so the notice states the restart
    /// requirement instead of implying an instant effect.
    pub(crate) fn update_local_config(
        &mut self,
        apply: impl FnOnce(&mut crate::app_config::AppConfig) -> String,
    ) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        let label = apply(&mut config);
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.app.notice.set(format!("{label} (restart applies)"));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(crate) fn toggle_mouse_capture(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        config.behavior.mouse_capture = !config.behavior.mouse_capture;
        let enabled = config.behavior.mouse_capture;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.perf_policy.enable_mouse_capture = enabled;
                if !enabled {
                    // Capture off must leave no armed press, chain, hover
                    // or gutter drag behind for the next enable to trip on.
                    self.mouse_press = None;
                    self.clear_click_chain();
                    self.clear_mouse_hover();
                    if let Some(session) = self.sessions.active_mut() {
                        session.end_gutter_drag();
                    }
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app.notice.set(format!(
                    "mouse_capture {} (applies instantly)",
                    if enabled { "on" } else { "off" }
                ));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(crate) fn clear_selection_after_copy(&self) -> bool {
        matches!(
            self.copy_on_select,
            crate::app_config::CopyOnSelectMode::CopyAndClear
        )
    }

    pub(crate) fn copy_on_select_enabled(&self) -> bool {
        !matches!(
            self.copy_on_select,
            crate::app_config::CopyOnSelectMode::Off
        )
    }

    pub(crate) fn toggle_copy_on_select(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        use crate::app_config::CopyOnSelectMode;
        config.behavior.copy_on_select = match config.behavior.copy_on_select {
            CopyOnSelectMode::Off => CopyOnSelectMode::Copy,
            CopyOnSelectMode::Copy => CopyOnSelectMode::CopyAndClear,
            CopyOnSelectMode::CopyAndClear => CopyOnSelectMode::Off,
        };
        let mode = config.behavior.copy_on_select;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.copy_on_select = mode;
                let clear = self.clear_selection_after_copy();
                for tab in self.sessions.iter_mut() {
                    tab.controller.apply_clear_selection_after_copy(clear);
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app
                    .notice
                    .set(format!("copy_on_select {mode:?} (applies instantly)"));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(crate) fn toggle_session_scrollbar(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        config.behavior.session_scrollbar = !config.behavior.session_scrollbar;
        let enabled = config.behavior.session_scrollbar;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                for tab in self.sessions.iter_mut() {
                    tab.controller.set_session_scrollbar(enabled);
                }
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app.notice.set(format!(
                    "session_scrollbar {} (applies instantly)",
                    if enabled { "on" } else { "off" }
                ));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    pub(crate) fn toggle_sidebar(&mut self) {
        let Some(path) = crate::app_config::config_file_path() else {
            self.app.notice.set("Config path unavailable.".to_string());
            return;
        };
        let mut config = crate::app_config::ConfigManager::load(&path)
            .map(|manager| manager.config().clone())
            .unwrap_or_default();
        config.behavior.sidebar = !config.behavior.sidebar;
        let enabled = config.behavior.sidebar;
        match crate::app_config::ConfigManager::save(&path, &config) {
            Ok(()) => {
                self.invalidate(ScreenKind::Settings);
                self.sidebar = enabled;
                self.pending_scope.request(crate::redraw::RedrawScope::Full);
                self.app.notice.set(format!(
                    "sidebar {} (applies instantly)",
                    if enabled { "on" } else { "off" }
                ));
            }
            Err(err) => self.app.notice.set(format!("Save failed: {err}")),
        }
    }

    /// Cycle the double-Esc action: tree -> fork -> off -> tree. Applies
    /// instantly because sessions read the config on every Esc press.
    pub(crate) fn cycle_double_escape(&mut self) {
        use crate::app_config::DoubleEscapeActionName;
        self.update_local_config(|config| {
            let next = match config.behavior.double_escape_action() {
                DoubleEscapeActionName::Tree => DoubleEscapeActionName::Fork,
                DoubleEscapeActionName::Fork => DoubleEscapeActionName::Off,
                DoubleEscapeActionName::Off => DoubleEscapeActionName::Tree,
            };
            config.behavior.double_escape_action = Some(next);
            let label = match next {
                DoubleEscapeActionName::Tree => "tree",
                DoubleEscapeActionName::Fork => "fork",
                DoubleEscapeActionName::Off => "off",
            };
            format!("double-esc {label} (empty-composer double-Esc)")
        });
    }

    pub(crate) fn cycle_copy_target(&mut self) {
        use crate::app_config::CopyTargetName;
        self.update_local_config(|config| {
            let next = match config.terminal.copy_target {
                Some(CopyTargetName::Code) => None,
                _ => Some(CopyTargetName::Code),
            };
            config.terminal.copy_target = next;
            format!(
                "copy_target {}",
                if next.is_some() { "code" } else { "message" }
            )
        });
    }

    pub(crate) fn toggle_tool_output(&mut self) {
        self.update_local_config(|config| {
            let on = !config.tool_output.enabled.unwrap_or(true);
            config.tool_output.enabled = Some(on);
            format!("tool_output {}", if on { "on" } else { "off" })
        });
    }

    pub(crate) fn cycle_tool_level(&mut self) {
        use crate::app_config::ToolDetailLevelName;
        self.update_local_config(|config| {
            let next = match config.tool_output.level {
                Some(ToolDetailLevelName::Low) => ToolDetailLevelName::Medium,
                Some(ToolDetailLevelName::Medium) => ToolDetailLevelName::High,
                _ => ToolDetailLevelName::Low,
            };
            config.tool_output.level = Some(next);
            let label = match next {
                ToolDetailLevelName::Low => "low",
                ToolDetailLevelName::Medium => "medium",
                ToolDetailLevelName::High => "high",
            };
            format!("tool_output level {label}")
        });
    }

    pub(crate) fn step_preview_budget(&mut self, up: bool) {
        self.update_local_config(|config| {
            let lines = step_option(
                config.tool_output.preview_max_lines.unwrap_or(10),
                &[5, 10, 20, 40],
                up,
            );
            let chars = step_option(
                config.tool_output.preview_max_chars.unwrap_or(320),
                &[160, 320, 640, 1280],
                up,
            );
            config.tool_output.preview_max_lines = Some(lines);
            config.tool_output.preview_max_chars = Some(chars);
            format!("preview {lines} lines · {chars} chars")
        });
    }
}
