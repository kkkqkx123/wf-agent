//! Keyboard input handling for the interactive session controller.
//!
//! One `impl` block over [`InteractiveController`] covering the input
//! responsibility: dispatching a key to the active footer view (prompt,
//! approval, or question), the always-available scrollback scrolling, and the
//! Ctrl-C double-press exit. These methods mutate controller state directly;
//! rendering and event draining live in the sibling modules.

use serde_json::Value;

use wf_api::ToolApprovalResult;

use crate::approval_overlay::ApprovalChoice;
use crate::footer::FooterView;
use crate::interactive::{InteractiveAction, InteractiveController};
use crate::keymap::{CKey, Key};
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

        // Scrolling belongs to the scrollback regardless of the active footer
        // view (a prompt, an approval or a question all leave history above).
        match key.code {
            CKey::PageUp => {
                self.scroll_history_up();
                return InteractiveAction::Continue;
            }
            CKey::PageDown => {
                self.view_scroll = self.view_scroll.saturating_sub(10);
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

    /// Scroll the scrollback viewport up by one page. When the viewport is
    /// already pinned to the oldest loaded row and the replay is `Partial`,
    /// request the next (older) page instead: the only way to reveal history
    /// behind the loaded window is to load it.
    fn scroll_history_up(&mut self) {
        if self.scroll_at_top {
            if self.pager.is_partial() {
                self.request_earlier_page();
            }
            return;
        }
        self.view_scroll = self.view_scroll.saturating_add(10);
    }

    fn handle_approval_key(&mut self, key: Key) -> InteractiveAction {
        let choice = match key.code {
            CKey::Char('y') => Some(ApprovalChoice::Approve),
            CKey::Char('a') => Some(ApprovalChoice::ApproveAll),
            CKey::Char('d') => Some(ApprovalChoice::DenyOnce),
            CKey::Char('n') => Some(ApprovalChoice::Deny),
            CKey::Char('c') | CKey::Esc => Some(ApprovalChoice::Cancel),
            _ => None,
        };
        if let Some(choice) = choice {
            let result = self.resolve_approval(choice);
            if let Some(tx) = self.approval_reply.take() {
                let _ = tx.send(result);
            }
            if let Some(remembered) = choice.remembered() {
                if let Some(view) = self.footer.approval.take() {
                    self.remembered
                        .remember(&view.request().tool_name, remembered);
                }
            }
            self.footer.present(FooterView::Prompt);
        }
        InteractiveAction::Continue
    }

    fn resolve_approval(&self, choice: ApprovalChoice) -> ToolApprovalResult {
        self.footer
            .approval
            .as_ref()
            .map(|view| view.apply(choice))
            .unwrap_or_else(|| ToolApprovalResult::rejected("", "no approval view"))
    }

    fn handle_question_key(&mut self, key: Key) -> InteractiveAction {
        let Some(question) = self.footer.question.as_mut() else {
            self.footer.present(FooterView::Prompt);
            return InteractiveAction::Continue;
        };
        match key.code {
            CKey::Esc => {
                let outcome = question.cancel();
                self.finish_question(&outcome);
            }
            CKey::Enter => {
                let outcome = question.submit();
                self.finish_question(&outcome);
            }
            CKey::Char(c) if c.is_ascii_digit() && !key.ctrl && !key.alt => {
                let _ = question.pick(c.to_digit(10).unwrap_or(0) as u8);
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
        self.pending_scroll
            .push(HistoryLine::new_role(format!("❯ {answer}"), Role::Accent));
        self.send_question_reply(&interaction_id, response);
        self.footer.present(FooterView::Prompt);
    }

    fn send_question_reply(&self, interaction_id: &str, response: Value) {
        if interaction_id.is_empty() {
            return;
        }
        let storage = self.adapter.api_context().storage.clone();
        let id = interaction_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = wf_api::entity::user_interaction::respond_interaction(
                &storage,
                &id,
                Some(response),
                None,
            )
            .await
            {
                tracing::warn!(target: "wf_cli", error = %err, "question respond failed");
            }
        });
    }

    fn handle_prompt_key(&mut self, key: Key) -> InteractiveAction {
        match key.code {
            CKey::Enter => {
                let text = self.footer.composer.submit().unwrap_or_default();
                if !text.trim().is_empty() {
                    self.pending_scroll
                        .push(HistoryLine::new_role(format!("❯ {text}"), Role::Accent));
                    self.start_turn(text.trim().to_string(), None, None);
                }
            }
            CKey::Backspace => self.footer.composer.backspace(),
            CKey::Delete => self.footer.composer.delete_forward(),
            CKey::Left => self.footer.composer.move_left(),
            CKey::Right => self.footer.composer.move_right(),
            CKey::Home => self.footer.composer.home(),
            CKey::End => self.footer.composer.end(),
            CKey::Char(c) if !key.ctrl && !key.alt => self.footer.composer.insert_char(c),
            _ => {}
        }
        InteractiveAction::Continue
    }
}

#[cfg(test)]
mod tests {
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
}
