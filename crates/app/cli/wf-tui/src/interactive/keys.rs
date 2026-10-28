//! Keyboard input handling for the interactive session controller.
//!
//! One `impl` block over [`InteractiveController`] covering the input
//! responsibility: dispatching a key to the active footer view (prompt,
//! approval, or question), the always-available scrollback scrolling, and the
//! Ctrl-C double-press exit. These methods mutate controller state directly;
//! rendering and event draining live in the sibling modules.

use serde_json::Value;

use tui_core::panel_select::Motion;

use crate::approval_overlay::ApprovalChoice;
use crate::footer::{FooterRoute, FooterView};
use crate::interactive::anchors::{first_line_label, AnchorKind};
use crate::interactive::{InteractiveAction, InteractiveController};
use crate::keymap::{CKey, Key, KeyAction, KeymapContext};
use crate::question_overlay::QuestionOutcome;
use crate::terminal::PressOutcome;
use crate::transcript::{HistoryLine, Role};

impl InteractiveController {
    /// Handle one key while the session screen is active.
    pub fn handle_key(&mut self, key: Key) -> InteractiveAction {
        if key.ctrl && key.code == CKey::Char('c') {
            let now_ms = self.now_ms();
            return match self.exit_tracker.press(now_ms) {
                PressOutcome::SecondPress => InteractiveAction::Exit,
                PressOutcome::FirstPress => {
                    self.pending_scroll.push(HistoryLine::new_role(
                        "Press Ctrl-C again within 5s to exit the session.".to_string(),
                        Role::Warning,
                    ));
                    InteractiveAction::Continue
                }
            };
        }

        // Keyboard selection works without mouse capture: shift chords move
        // a logical selection cursor over the transcript, Ctrl+A selects the
        // whole transcript and Esc drops an existing selection. These run
        // before the scroll and view dispatch so Shift+PageUp/PageDown grow
        // the selection instead of paging the viewport alone.
        if self.transcript_selection_keys_active() {
            if let Some(action) = self.handle_transcript_selection_key(key) {
                return action;
            }
        }

        // Scrolling belongs to the scrollback regardless of the active footer
        // view (a prompt, an approval or a question all leave history above).
        // Tool-output keys are transcript context too: Ctrl+O toggles the
        // most recent expandable block, Ctrl+V opens its full output,
        // Ctrl+D cycles the detail level, Ctrl+E expands or collapses all,
        // Ctrl+G jumps between sub-agent start rows newest-first, Ctrl+P
        // cycles user prompts, Ctrl+U cycles todo updates, Ctrl+F cycles
        // tool failures, Ctrl+N cycles checkpoints, Ctrl+X cycles human
        // decisions, Ctrl+I lists the anchor timeline, Alt+N/Alt+P move to
        // the next/previous anchor.
        // High level expands retrieval bodies by default and can flood the
        // transcript on large reads; prefer medium for daily use and switch
        // to high only when inspecting retrieval behavior.
        match key.code {
            CKey::End if key.ctrl && !key.alt => {
                self.jump_to_latest();
                return InteractiveAction::Continue;
            }
            CKey::Home if key.ctrl && !key.alt => {
                self.jump_to_oldest();
                return InteractiveAction::Continue;
            }
            CKey::End if !key.ctrl && !key.alt => {
                if self.footer.view != FooterView::Prompt
                    || !self.footer.route.owns_composer_input()
                    || (!self.footer.composer.search_active() && self.footer.composer.is_empty())
                {
                    self.jump_to_latest();
                    return InteractiveAction::Continue;
                }
            }
            CKey::PageUp => {
                self.scroll_history_up();
                return InteractiveAction::Continue;
            }
            CKey::PageDown => {
                self.scroll_history_down();
                return InteractiveAction::Continue;
            }
            CKey::Char('o') | CKey::Char('O') if key.ctrl && !key.alt => {
                self.toggle_last_tool_block();
                return InteractiveAction::Continue;
            }
            CKey::Char('v') | CKey::Char('V') if key.ctrl && !key.alt => {
                self.request_last_tool_viewer();
                return InteractiveAction::Continue;
            }
            CKey::Char('d') | CKey::Char('D') if key.ctrl && !key.alt => {
                self.cycle_detail_level();
                return InteractiveAction::Continue;
            }
            CKey::Char('e') | CKey::Char('E') if key.ctrl && !key.alt => {
                self.toggle_all_tool_blocks();
                return InteractiveAction::Continue;
            }
            CKey::Char('g') | CKey::Char('G') if key.ctrl && !key.alt => {
                self.jump_to_subagent();
                return InteractiveAction::Continue;
            }
            CKey::Char('p') | CKey::Char('P') if key.ctrl && !key.alt => {
                self.jump_to_anchor_kind(AnchorKind::UserPrompt);
                return InteractiveAction::Continue;
            }
            CKey::Char('u') | CKey::Char('U') if key.ctrl && !key.alt => {
                self.jump_to_anchor_kind(AnchorKind::TodoUpdate);
                return InteractiveAction::Continue;
            }
            CKey::Char('f') | CKey::Char('F') if key.ctrl && !key.alt => {
                self.jump_to_anchor_kind(AnchorKind::ToolFailure);
                return InteractiveAction::Continue;
            }
            CKey::Char('n') | CKey::Char('N') if key.ctrl && !key.alt => {
                self.jump_to_anchor_kind(AnchorKind::Checkpoint);
                return InteractiveAction::Continue;
            }
            CKey::Char('x') | CKey::Char('X') if key.ctrl && !key.alt => {
                self.jump_to_anchor_kind(AnchorKind::Decision);
                return InteractiveAction::Continue;
            }
            CKey::Char('i') | CKey::Char('I') if key.ctrl && !key.alt => {
                self.list_anchors();
                return InteractiveAction::Continue;
            }
            CKey::Char('n') | CKey::Char('N') if key.alt && !key.ctrl => {
                self.jump_to_next_anchor();
                return InteractiveAction::Continue;
            }
            CKey::Char('p') | CKey::Char('P') if key.alt && !key.ctrl => {
                self.jump_to_prev_anchor();
                return InteractiveAction::Continue;
            }
            // Cycle the nested agent-loop selection without leaving the
            // composer, mirroring the anchor-jump shortcut style.
            CKey::Char('l') | CKey::Char('L') if key.alt && !key.ctrl => {
                self.cycle_nested_target();
                return InteractiveAction::Continue;
            }
            CKey::Char('y') | CKey::Char('Y') if key.ctrl && !key.alt => {
                self.request_copy_default();
                return InteractiveAction::Continue;
            }
            _ => {}
        }

        match self.footer.view {
            FooterView::Permission => self.handle_approval_key(key),
            FooterView::Question => self.handle_question_key(key),
            FooterView::Prompt => self.handle_prompt_key(key),
        }
    }

    /// Route one key to transcript selection when it uses a selection chord.
    /// Returns `None` for every other key so ordinary scrolling, prompt
    /// editing and panel navigation stay untouched.
    fn handle_transcript_selection_key(&mut self, key: Key) -> Option<InteractiveAction> {
        if key.ctrl && !key.alt && matches!(key.code, CKey::Char('a') | CKey::Char('A')) {
            self.select_all_transcript();
            return Some(InteractiveAction::Continue);
        }
        if key.ctrl && !key.alt && matches!(key.code, CKey::Char('y') | CKey::Char('Y')) {
            if self.has_transcript_selection() {
                self.copy_transcript_selection();
                return Some(InteractiveAction::Continue);
            }
            return None;
        }
        if key.shift && !key.ctrl && !key.alt {
            let motion = match key.code {
                CKey::Left => Motion::Left,
                CKey::Right => Motion::Right,
                CKey::Up => Motion::Up,
                CKey::Down => Motion::Down,
                CKey::Home => Motion::LineStart,
                CKey::End => Motion::LineEnd,
                CKey::PageUp => Motion::PageUp(self.transcript_page_rows()),
                CKey::PageDown => Motion::PageDown(self.transcript_page_rows()),
                _ => return None,
            };
            self.extend_transcript_selection(motion);
            return Some(InteractiveAction::Continue);
        }
        if key.code == CKey::Esc && self.has_transcript_selection() {
            self.clear_transcript_selection();
            return Some(InteractiveAction::Continue);
        }
        None
    }

    /// Scroll the scrollback viewport up by one page. When the viewport is
    /// already pinned to the oldest loaded row and the replay is `Partial`,
    /// request the next (older) page instead: the only way to reveal history
    /// behind the loaded window is to load it. Taking manual control abandons
    /// streaming catch-up through a soft invalidation only.
    pub fn scroll_history_up(&mut self) {
        if self.scroll_at_top {
            if self.pager.is_partial() {
                self.request_earlier_page();
            }
            return;
        }
        self.tail_lag = 0;
        self.soft_invalidate();
        self.view_scroll = self.view_scroll.saturating_add(10);
    }

    /// Scroll the scrollback viewport down by one page towards the live tail.
    pub fn scroll_history_down(&mut self) {
        self.tail_lag = 0;
        self.soft_invalidate();
        self.view_scroll = self.view_scroll.saturating_sub(10);
    }

    /// Jump back to the live tail after reading history. Explicit navigation
    /// snaps instantly, abandoning any catch-up slide.
    pub fn jump_to_latest(&mut self) {
        self.view_scroll = 0;
        self.tail_lag = 0;
        self.scroll_at_top = false;
        self.soft_invalidate();
    }

    /// Jump to the oldest loaded row. The offset is pinned beyond the top
    /// and clamped to the laid-out maximum on the next draw, matching the
    /// `End` sentinel convention on the settings screen. A repeat press
    /// while pinned pages earlier history when the replay is partial.
    pub fn jump_to_oldest(&mut self) {
        let was_at_top = self.scroll_at_top;
        self.view_scroll = usize::MAX;
        self.tail_lag = 0;
        self.scroll_at_top = true;
        self.soft_invalidate();
        if was_at_top && self.pager.is_partial() {
            self.request_earlier_page();
        }
    }

    /// Insert a bracketed-paste body into the prompt composer as one whole
    /// string. Only the routes that own the composer accept pastes: the
    /// composer route and the composite mention route. Approval and
    /// question views own their keys, and the exclusive panel routes show
    /// selection lists with no text entry. Returns whether the paste landed.
    pub fn insert_paste(&mut self, text: &str) -> bool {
        if self.footer.view != FooterView::Prompt || !self.footer.route.owns_composer_input() {
            return false;
        }
        let normalized = normalize_paste(text);
        if normalized.is_empty() {
            return false;
        }
        self.footer.composer.insert_text(&normalized);
        self.sync_mention_route();
        true
    }

    fn handle_approval_key(&mut self, key: Key) -> InteractiveAction {
        let Some(choice) = self
            .keymap
            .resolve_action(KeymapContext::Approval, key)
            .and_then(ApprovalChoice::from_action)
        else {
            return InteractiveAction::Continue;
        };
        let answer = self.resolve_approval(choice);
        if let Some(id) = self.pending_approval.take() {
            self.post_approval_answer(&id, answer);
        }
        if let Some(remembered) = choice.remembered() {
            if let Some(view) = self.footer.approval.take() {
                self.remembered
                    .remember(&view.request().tool_name, remembered);
            }
        }
        self.footer.present(FooterView::Prompt);
        InteractiveAction::Continue
    }

    fn resolve_approval(&self, choice: ApprovalChoice) -> crate::approval_overlay::ApprovalAnswer {
        self.footer
            .approval
            .as_ref()
            .map(|view| view.apply(choice))
            .unwrap_or_else(|| {
                crate::approval_overlay::ApprovalAnswer::rejected("no approval view")
            })
    }

    fn handle_question_key(&mut self, key: Key) -> InteractiveAction {
        let Some(question) = self.footer.question.as_ref() else {
            self.footer.present(FooterView::Prompt);
            return InteractiveAction::Continue;
        };
        let Some(action) = self.keymap.resolve_action(KeymapContext::Question, key) else {
            return InteractiveAction::Continue;
        };
        match action {
            KeyAction::Cancel | KeyAction::Back => {
                let outcome = question.cancel();
                self.finish_question(&outcome);
            }
            KeyAction::Select | KeyAction::Submit => {
                let outcome = question.submit();
                self.finish_question(&outcome);
            }
            KeyAction::Pick(index) => {
                if let Some(question) = self.footer.question.as_mut() {
                    let _ = question.pick(index);
                }
            }
            _ => {}
        }
        InteractiveAction::Continue
    }

    fn finish_question(&mut self, outcome: &QuestionOutcome) {
        let Some(question) = self.footer.question.take() else {
            return;
        };
        let answer = question.answer_text(outcome);
        let response = question.response_value(outcome);
        let interaction_id = question.interaction_id().to_string();
        self.flush_activity();
        let mut rows = vec![HistoryLine::new_role(format!("◆ {answer}"), Role::Accent)];
        self.assign_seq(&mut rows);
        let seq = rows.first().map(|row| row.seq_no()).unwrap_or(0);
        self.record_anchor(
            AnchorKind::Decision,
            &format!("decision-{seq}"),
            first_line_label(&answer, 60),
            seq,
        );
        self.pending_scroll.extend(rows);
        self.send_question_reply(&interaction_id, response);
        self.footer.present(FooterView::Prompt);
    }

    fn send_question_reply(&self, interaction_id: &str, response: Value) {
        if interaction_id.is_empty() {
            return;
        }
        let client = self.client.clone();
        let id = interaction_id.to_string();
        let body = serde_json::json!({"response_data": response});
        tokio::spawn(async move {
            if let Err(err) = client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/interactions/{id}/respond"),
                    &body,
                )
                .await
            {
                tracing::warn!(target: "wf_cli", error = %err, "question respond failed");
            }
        });
    }

    fn handle_prompt_key(&mut self, key: Key) -> InteractiveAction {
        // Panel routes own the keys: the panel branch runs before the
        // composer branch so an open panel never leaks text into the
        // composer.
        if self.footer.route != FooterRoute::Composer {
            return self.handle_panel_key(key);
        }
        // An open reverse search owns the prompt keys first (menu/panel
        // routes keep their own Up/Down; approval/question never reach here).
        if self.footer.composer.search_active() {
            if key.ctrl && !key.alt && matches!(key.code, CKey::Char('r') | CKey::Char('R')) {
                self.footer.composer.search_older();
                return InteractiveAction::Continue;
            }
            match key.code {
                CKey::Up if !key.ctrl && !key.alt && !key.shift => {
                    if self.footer.composer.search_match_count() > 0 {
                        self.footer.composer.search_older();
                    } else {
                        self.footer.composer.search_accept();
                        self.footer.composer.history_prev();
                    }
                    return InteractiveAction::Continue;
                }
                CKey::Down if !key.ctrl && !key.alt && !key.shift => {
                    if self.footer.composer.search_match_count() > 0 {
                        self.footer.composer.search_newer();
                    } else {
                        self.footer.composer.search_accept();
                        self.footer.composer.history_next();
                    }
                    return InteractiveAction::Continue;
                }
                CKey::Enter if !key.ctrl && !key.alt => {
                    // Accept only; a second Enter sends.
                    self.footer.composer.search_accept();
                    return InteractiveAction::Continue;
                }
                CKey::Esc => {
                    self.footer.composer.search_cancel();
                    return InteractiveAction::Continue;
                }
                CKey::Backspace if !key.ctrl && !key.alt => {
                    self.footer.composer.search_backspace();
                    return InteractiveAction::Continue;
                }
                CKey::Delete if !key.ctrl && !key.alt => {
                    return InteractiveAction::Continue;
                }
                CKey::Char(c) if !key.ctrl && !key.alt => {
                    self.footer.composer.search_push_char(c);
                    return InteractiveAction::Continue;
                }
                _ => {
                    // Any other key takes the lit entry, then does its
                    // ordinary job against the accepted text.
                    self.footer.composer.search_accept();
                }
            }
        }
        match key.code {
            CKey::Enter => {
                // Shift+Enter inserts a newline inside the input; plain Enter
                // and Ctrl+Enter submit. Without keyboard enhancement the
                // terminal never reports the shift modifier, so behavior
                // falls back to plain submit automatically.
                if key.shift && !key.ctrl && !key.alt {
                    self.footer.composer.insert_text("\n");
                    return InteractiveAction::Continue;
                }
                let text = self.footer.composer.submit().unwrap_or_default();
                if self.handle_copy_command(&text) {
                    return InteractiveAction::Continue;
                }
                if self.handle_command_line(&text) {
                    return InteractiveAction::Continue;
                }
                if !text.trim().is_empty() {
                    self.flush_activity();
                    let mut rows = vec![HistoryLine::new_role(format!("❯ {text}"), Role::Accent)];
                    self.assign_seq(&mut rows);
                    let seq = rows.first().map(|row| row.seq_no()).unwrap_or(0);
                    self.record_anchor(
                        AnchorKind::UserPrompt,
                        &format!("prompt-{seq}"),
                        first_line_label(text.trim(), 60),
                        seq,
                    );
                    self.pending_scroll.extend(rows);
                    self.submit_prompt(text.trim().to_string());
                }
            }
            CKey::Up if !key.ctrl && !key.alt && !key.shift => {
                self.move_composer_up();
            }
            CKey::Down if !key.ctrl && !key.alt && !key.shift => {
                self.move_composer_down();
            }
            CKey::Right if !key.ctrl && !key.alt => {
                if !self.footer.composer.accept_ghost() {
                    self.footer.composer.move_right();
                }
            }
            CKey::End if !key.ctrl && !key.alt => {
                if !self.footer.composer.accept_ghost() {
                    self.footer.composer.end();
                }
            }
            CKey::Char('r') | CKey::Char('R') if key.ctrl && !key.alt => {
                self.footer.composer.begin_search();
            }
            CKey::Backspace => {
                self.footer.composer.backspace();
                self.sync_mention_route();
            }
            CKey::Delete => {
                self.footer.composer.delete_forward();
                self.sync_mention_route();
            }
            CKey::Left => self.footer.composer.move_left(),
            CKey::Home => self.footer.composer.home(),
            CKey::Char(c) if !key.ctrl && !key.alt => {
                // `/` opens the command palette when the composer is empty;
                // anywhere else it is ordinary text.
                if c == '/' && self.footer.composer.is_empty() {
                    self.open_command_panel();
                    return InteractiveAction::Continue;
                }
                self.footer.composer.insert_char(c);
                self.sync_mention_route();
            }
            _ => {}
        }
        InteractiveAction::Continue
    }

    /// Shared Up semantics: single-row input keeps legacy pure history;
    /// multiline input moves the cursor until the first visual row, jumps
    /// to the buffer start, then walks history.
    fn move_composer_up(&mut self) {
        let width = self.last_layout_width.max(1);
        if self.footer.composer.visual_row_count(width) <= 1 {
            self.footer.composer.history_prev();
            return;
        }
        if !self.footer.composer.is_on_first_visual_row(width) {
            self.footer.composer.move_up(width);
            return;
        }
        if !self.footer.composer.is_at_start() {
            self.footer.composer.home();
            return;
        }
        self.footer.composer.history_prev();
    }

    /// Shared Down semantics, symmetric to [`Self::move_composer_up`].
    fn move_composer_down(&mut self) {
        let width = self.last_layout_width.max(1);
        if self.footer.composer.visual_row_count(width) <= 1 {
            self.footer.composer.history_next();
            return;
        }
        if !self.footer.composer.is_on_last_visual_row(width) {
            self.footer.composer.move_down(width);
            return;
        }
        if !self.footer.composer.is_at_end() {
            self.footer.composer.end();
            return;
        }
        self.footer.composer.history_next();
    }
}

/// Normalize a bracketed-paste body for insertion: terminals wrap lines as
/// CRLF, and the composer treats LF as the row separator.
pub(crate) fn normalize_paste(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::normalize_paste;
    use crate::approval_overlay::ApprovalChoice;
    use crate::keymap::{CKey, Key};

    #[test]
    fn approval_choice_from_key() {
        assert_eq!(
            ApprovalChoice::from_action(crate::keymap::KeyAction::Approve),
            Some(ApprovalChoice::Approve)
        );
        assert!(Key::ctrl(CKey::Char('c')).ctrl);
    }

    #[test]
    fn paste_normalization_folds_carriage_returns() {
        assert_eq!(normalize_paste("a\r\nb\rc"), "a\nb\nc");
        assert_eq!(normalize_paste("plain"), "plain");
        assert_eq!(normalize_paste(""), "");
    }
}
