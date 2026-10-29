//! Explicit copy actions for the interactive session controller.
//!
//! Key and `/copy` command handling resolves through the shared pure
//! functions in `tui-core::copy`: the last contiguous assistant segment
//! of the scrollback is the message source, fenced code blocks are
//! extracted from it, and the resolved payload waits as a pending request
//! the shell flushes after the next frame. Failures wait as notices the
//! shell surfaces; nothing here touches the terminal.

use crate::copy::{
    last_assistant_text, resolve_copy_text, spec_or_default, CopyPolicy, CopyResolveError,
    CopySourceLine, PendingCopy,
};
use crate::transcript::Role;

use super::InteractiveController;

impl InteractiveController {
    /// Inject the startup-resolved copy policy.
    pub fn apply_copy_policy(&mut self, policy: CopyPolicy) {
        self.copy_policy = policy;
    }

    /// Inject the selection copy clear behaviour.
    pub fn apply_clear_selection_after_copy(&mut self, clear: bool) {
        self.clear_selection_after_copy = clear;
    }

    /// Drain a queued copy payload, if any.
    pub fn take_pending_copy(&mut self) -> Option<PendingCopy> {
        self.pending_copy.take()
    }

    /// Drain a queued copy failure notice, if any.
    pub fn take_copy_notice(&mut self) -> Option<String> {
        self.pending_copy_notice.take()
    }

    /// Copy with the configured default target (the Ctrl+Y key).
    pub fn request_copy_default(&mut self) {
        self.request_copy_with_arg("");
    }

    /// Copy with an explicit `/copy` argument.
    pub fn request_copy_with_arg(&mut self, arg: &str) {
        if !self.copy_policy.allowed {
            self.pending_copy_notice = Some(self.copy_policy.deny_notice().to_string());
            return;
        }
        let spec = match spec_or_default(arg, self.copy_policy.default_target) {
            Ok(spec) => spec,
            Err(err) => {
                self.pending_copy_notice = Some(err.notice());
                return;
            }
        };
        let assistant = self.last_assistant_source_text();
        let session = self.session_id().map(str::to_string);
        match resolve_copy_text(spec, assistant.as_deref(), session.as_deref()) {
            Ok((text, label)) => match crate::terminal::sanitize_copy_text(&text) {
                Some(clean) => {
                    self.pending_copy = Some(PendingCopy::new(clean, label));
                }
                None => {
                    self.pending_copy_notice = Some(CopyResolveError::EmptyReply.notice());
                }
            },
            Err(err) => {
                self.pending_copy_notice = Some(err.notice());
            }
        }
    }

    /// Whether `text` is a `/copy` command line.
    pub fn is_copy_command(text: &str) -> bool {
        let trimmed = text.trim();
        trimmed == "/copy" || trimmed.starts_with("/copy ") || trimmed.starts_with("/copy\t")
    }

    /// Handle a `/copy...` composer line. Returns true when consumed.
    pub fn handle_copy_command(&mut self, text: &str) -> bool {
        if !Self::is_copy_command(text) {
            return false;
        }
        let arg = text.trim().strip_prefix("/copy").unwrap_or("").trim();
        self.request_copy_with_arg(arg);
        true
    }

    /// Source text of the last contiguous assistant segment over the
    /// settled scrollback plus queued (not yet settled) rows. In-flight
    /// streaming text is excluded: only committed rows are copyable.
    fn last_assistant_source_text(&self) -> Option<String> {
        let lines: Vec<CopySourceLine> = self
            .scrollback_rows_for_copy()
            .iter()
            .map(|line| CopySourceLine {
                text: line.source_text(),
                is_assistant: line.role == Role::Default,
            })
            .collect();
        last_assistant_text(&lines)
    }

    /// Rows visible to copy: settled scrollback first, then queued rows.
    fn scrollback_rows_for_copy(&self) -> Vec<crate::transcript::HistoryLine> {
        let mut rows = self.scrollback.clone();
        rows.extend(self.pending_scroll.clone());
        rows
    }
}
