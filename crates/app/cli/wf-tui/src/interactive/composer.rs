//! Interactive composer: cursor, copy text and staged removal.

use super::*;
use crate::animation::AnimationController;
use crate::approval_overlay::ApprovalRemembered;
use crate::context_sidebar::{ContextSnapshot, SidebarSnapshot, TodoLine};
use crate::footer::Footer;
use crate::pager::ReplayPager;
use crate::prep_cache::PreparedScrollback;
use crate::reducer::Phase;
use crate::reducer::SessionReducer;
use crate::stream_pacer::SegmentedPacer;
use crate::terminal::{DoublePressTracker, ESC_DOUBLE_PRESS_WINDOW, SIGINT_DOUBLE_PRESS_WINDOW};
use crate::transcript::HistoryLine;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use wf_client::WfClient;
use wf_types::events::ExecutionStreamEvent;
use wf_types::interaction::tool_approval::ToolApprovalRequestData;

impl InteractiveController {
    /// Move the composer cursor to a click cell. Returns whether the cursor
    /// moved and a frame is due.
    pub fn move_composer_cursor_to_cell(&mut self, col: u16, row: u16) -> bool {
        let Some(area) = self.last_input_area else {
            return false;
        };
        let Some(byte) = self.footer.composer.byte_at_input_cell(area, col, row) else {
            return false;
        };
        self.footer.composer.set_cursor(byte)
    }

    /// Full composer buffer for triple-click copy.
    pub fn composer_text(&self) -> String {
        self.footer.composer.content().to_string()
    }

    /// Clipboard text for the whole composer buffer. Every staged
    /// placeholder expands to its label plus a resolvable description
    /// while mentions stay verbatim, so cross-session pastes keep meaning.
    pub fn composer_copy_text(&self) -> String {
        self.expanded_composer_copy_text()
    }

    /// Expand one copied token when it matches a staged placeholder.
    /// Orphan labels stay verbatim; mentions are never rewritten here.
    pub(crate) fn expand_placeholder_copy(&self, token: &str) -> String {
        if let Some(describe) = self.describe_placeholder(token) {
            return format!("{token} ({describe})");
        }
        token.to_string()
    }

    /// Expand every staged placeholder in the buffer for clipboard use.
    /// Submit and history paths keep the raw buffer plus vectors.
    pub(crate) fn expanded_composer_copy_text(&self) -> String {
        let mut text = self.footer.composer.content().to_string();
        for span in self.footer.composer.attachment_spans() {
            let label = text[span.range.clone()].to_string();
            if let Some(describe) = self.describe_placeholder(&label) {
                let expanded = format!("{label} ({describe})");
                text = text.replacen(label.as_str(), expanded.as_str(), 1);
            }
        }
        text
    }

    /// Human description behind a staged placeholder label, if staged.
    pub(crate) fn describe_placeholder(&self, label: &str) -> Option<String> {
        if let Some(pos) = self.staged_image_labels.iter().position(|l| l == label) {
            return self
                .staged_images
                .get(pos)
                .map(wf_cli_shared::turn::describe_user_image);
        }
        if let Some(pos) = self
            .staged_attachment_labels
            .iter()
            .position(|l| l == label)
        {
            return self
                .staged_attachments
                .get(pos)
                .map(wf_cli_shared::turn::describe_user_attachment);
        }
        None
    }

    /// Remove one staged image by its 1-based list number. The matching
    /// placeholder leaves the composer together with its payload.
    pub fn remove_staged_image_at(&mut self, index: usize) -> String {
        if index == 0 || index > self.staged_images.len() {
            return format!("no staged image {index} (1..={})", self.staged_images.len());
        }
        let at = index - 1;
        self.staged_images.remove(at);
        let label = self.staged_image_labels.remove(at);
        self.footer
            .composer
            .remove_attachment_labels(std::slice::from_ref(&label));
        self.sync_mention_route();
        format!("removed staged image {index} {label}")
    }

    /// Remove one staged text file by its 1-based list number.
    pub fn remove_staged_attachment_at(&mut self, index: usize) -> String {
        if index == 0 || index > self.staged_attachments.len() {
            return format!(
                "no staged file {index} (1..={})",
                self.staged_attachments.len()
            );
        }
        let at = index - 1;
        self.staged_attachments.remove(at);
        let label = self.staged_attachment_labels.remove(at);
        self.footer
            .composer
            .remove_attachment_labels(std::slice::from_ref(&label));
        self.sync_mention_route();
        format!("removed staged file {index} {label}")
    }
}
