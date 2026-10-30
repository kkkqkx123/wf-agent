//! Copy / paste handling for the TUI shell: composer and transcript copy
//! helpers that queue OSC 52 payloads, the post-frame flush point and the
//! bracketed-paste router.

use std::io;

use crate::terminal::{CrosstermControl, TerminalControl, TerminalGuard};

use super::input_classify::{classify_paste, PasteTarget};
use super::TuiApp;

impl TuiApp {
    /// Flush one queued copy request. Local clipboard tools run first and
    /// OSC 52 stays the fallback, so headless and remote sessions keep their
    /// existing policy gate while local sessions stop depending on
    /// emulator pass-through. Runs after `draw` returns so terminal bytes
    /// always follow a complete frame. Denied or failed writes surface as
    /// notices and never touch state.
    pub(super) fn flush_pending_copy(
        &mut self,
        guard: &mut TerminalGuard<CrosstermControl<io::Stdout>>,
    ) {
        let Some(request) = self.pending_copy.take() else {
            return;
        };
        let Some(payload) = crate::terminal::sanitize_copy_text(&request.text) else {
            return;
        };
        if crate::clipboard::try_external_clipboard(&payload) {
            self.app
                .notice
                .set(crate::copy::success_notice(&request.label, &payload));
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        }
        if !self.copy_policy.allowed {
            self.app
                .notice
                .set(self.copy_policy.deny_notice().to_string());
            self.pending_scope.request(crate::redraw::RedrawScope::Full);
            return;
        }
        match guard.control().copy_to_clipboard(&payload) {
            Ok(()) => self
                .app
                .notice
                .set(crate::copy::success_notice(&request.label, &payload)),
            Err(err) => self.app.notice.set(format!("copy failed: {err}")),
        }
        self.pending_scope.request(crate::redraw::RedrawScope::Full);
    }

    /// Route a bracketed-paste body to the currently focused input as one
    /// whole string: no shortcut semantics fire on pasted content. Returns
    /// whether the paste landed somewhere visible.
    pub(super) fn handle_paste(&mut self, text: &str) -> bool {
        if !self.modals.is_empty() {
            return false;
        }
        if self.overlay == crate::overlay::OverlayMode::SearchPalette {
            let normalized = crate::interactive::keys::normalize_paste(text);
            if normalized.is_empty() {
                return false;
            }
            self.search_palette.append_paste(&normalized);
            return true;
        }
        if self.overlay == crate::overlay::OverlayMode::CommandPalette {
            let normalized = crate::interactive::keys::normalize_paste(text);
            if normalized.is_empty() {
                return false;
            }
            self.command_palette.append_paste(&normalized);
            return true;
        }
        match classify_paste(false, self.app.screen.navigation.current_kind()) {
            PasteTarget::Swallowed | PasteTarget::Ignored => false,
            PasteTarget::Session => {
                if let Some(session) = self.sessions.active_mut() {
                    session.insert_paste(text)
                } else {
                    false
                }
            }
        }
    }

    /// Copy the word at a visible display row and terminal column.
    pub(super) fn copy_word_at(&mut self, display_row: usize, col: u16) -> bool {
        let (text, display_col) = match self.sessions.active().and_then(|session| {
            let area = session.last_scroll_area()?;
            let text = session.plain_text_for_display_row(display_row)?;
            Some((text, col.saturating_sub(area.x) as usize))
        }) {
            Some(pair) => pair,
            None => return false,
        };
        let Some(word) = crate::interactive::mouse_hit::word_at_col(&text, display_col) else {
            return false;
        };
        self.queue_selection_copy(word)
    }

    /// Copy the full logical line behind a visible display row.
    pub(super) fn copy_line_at(&mut self, display_row: usize) -> bool {
        let Some(text) = self
            .sessions
            .active()
            .and_then(|session| session.source_text_for_display_row(display_row))
        else {
            return false;
        };
        self.queue_selection_copy(text)
    }

    /// Copy the logical transcript selection between its anchor and head,
    /// combined with any earlier Alt+Click regions. The ranges are cleared
    /// only after the payload is resolved.
    pub(super) fn copy_transcript_selection(
        &mut self,
        selection: crate::interactive::TranscriptSelection,
        clear_after: bool,
    ) -> bool {
        let text = {
            let session = match self.sessions.active_mut() {
                Some(session) => session,
                None => return false,
            };
            if clear_after {
                session.take_transcript_selection_text(Some(selection))
            } else {
                session.selected_transcript_text_all(Some(selection))
            }
        };
        let Some(text) = text else {
            return false;
        };
        self.queue_selection_copy(text)
    }

    /// Copy one word from the composer at a click cell.
    pub(super) fn copy_composer_word_at(&mut self, col: u16, row: u16) -> bool {
        let Some(word) = self
            .sessions
            .active()
            .and_then(|session| session.composer_word_at_cell(col, row))
        else {
            return false;
        };
        self.queue_selection_copy(word)
    }

    /// Copy the whole composer buffer.
    pub(super) fn copy_composer_text(&mut self) -> bool {
        let text = match self.sessions.active() {
            Some(session) => session.composer_text(),
            None => return false,
        };
        if text.trim().is_empty() {
            return false;
        }
        self.queue_selection_copy(text)
    }

    /// Queue one sanitized selection payload for the post-frame flush.
    /// Denials surface as a notice; an empty sanitized result copies nothing.
    pub(super) fn queue_selection_copy(&mut self, text: String) -> bool {
        if !self.copy_policy.allowed {
            self.app
                .notice
                .set(self.copy_policy.deny_notice().to_string());
            return true;
        }
        let Some(clean) = crate::terminal::sanitize_copy_text(&text) else {
            return false;
        };
        self.pending_copy = Some(crate::copy::PendingCopy::new(clean, "selection"));
        true
    }
}
