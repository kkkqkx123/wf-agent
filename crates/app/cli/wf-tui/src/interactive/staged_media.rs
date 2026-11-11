//! Interactive staged media: images, attachments and composer staging.

use super::status_dialog::parse_placeholder_index;
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
    /// List staged images for the next turn.
    pub fn staged_image_lines(&self) -> Vec<String> {
        if self.staged_images.is_empty() {
            return vec![
                format!(
                    "no staged images (usage: /image <path-or-url>...) [detail: {}]",
                    Self::image_detail_label(self.staged_image_detail)
                ),
                wf_cli_shared::turn::stage_budget_line(
                    &self.staged_images,
                    &self.staged_attachments,
                ),
            ];
        }
        let mut lines: Vec<String> = self
            .staged_images
            .iter()
            .enumerate()
            .map(|(index, image)| {
                format!(
                    "[{}] {} ({})",
                    index + 1,
                    image.media_type.as_deref().unwrap_or("image"),
                    wf_cli_shared::turn::describe_user_image(image)
                )
            })
            .collect();
        lines.push(format!(
            "detail for new stages: {}",
            Self::image_detail_label(self.staged_image_detail)
        ));
        lines.push(wf_cli_shared::turn::stage_budget_line(
            &self.staged_images,
            &self.staged_attachments,
        ));
        lines
    }

    /// List staged text files for the next turn.
    pub fn staged_attachment_lines(&self) -> Vec<String> {
        if self.staged_attachments.is_empty() {
            return vec![
                "no staged files (usage: session.attach <path>...)".to_string(),
                wf_cli_shared::turn::stage_budget_line(
                    &self.staged_images,
                    &self.staged_attachments,
                ),
            ];
        }
        let mut lines: Vec<String> = self
            .staged_attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| {
                format!(
                    "[{}] {}",
                    index + 1,
                    wf_cli_shared::turn::describe_user_attachment(attachment)
                )
            })
            .collect();
        lines.push(wf_cli_shared::turn::stage_budget_line(
            &self.staged_images,
            &self.staged_attachments,
        ));
        lines
    }

    pub(crate) fn image_detail_label(
        detail: Option<wf_types::message::ImageDetail>,
    ) -> &'static str {
        match detail {
            None => "provider default",
            Some(wf_types::message::ImageDetail::Auto) => "auto",
            Some(wf_types::message::ImageDetail::Low) => "low",
            Some(wf_types::message::ImageDetail::High) => "high",
            Some(wf_types::message::ImageDetail::Original) => "original",
        }
    }

    /// Show or set the render detail for newly staged images.
    pub fn set_tui_image_detail(&mut self, raw: Option<&str>) -> String {
        match raw.map(str::trim) {
            None | Some("") => format!("image detail: {}", Self::image_detail_label(self.staged_image_detail)),
            Some("clear") | Some("default") => {
                self.staged_image_detail = None;
                "image detail cleared (provider default)".to_string()
            }
            Some(value) => match wf_cli_shared::turn::parse_image_detail_arg(Some(value)) {
                Ok(detail) => {
                    self.staged_image_detail = detail;
                    format!("image detail: {}", Self::image_detail_label(self.staged_image_detail))
                }
                Err(err) => format!("invalid image detail: {err} (usage: /image-detail [auto|low|high|original|clear])"),
            },
        }
    }

    /// Stage local paths or remote URLs for the next turn. Failures leave
    /// the existing stage untouched; the combined stage is validated.
    /// Success inserts one visible `[Image N]` placeholder per new entry
    /// at the composer cursor, mirroring the opencode attach flow.
    pub fn stage_tui_images(&mut self, specs: &[String]) -> String {
        match wf_cli_shared::turn::load_user_image_specs(specs, self.staged_image_detail) {
            Err(err) => format!("cannot stage images: {err}"),
            Ok(loaded) => {
                let mut combined = self.staged_images.clone();
                combined.extend(loaded.clone());
                match wf_types::message::validate_user_images(&combined) {
                    Err(reason) => {
                        format!("cannot stage images: {reason} (stage unchanged)")
                    }
                    Ok(()) => {
                        let base = self.next_image_index();
                        let labels: Vec<String> = loaded
                            .iter()
                            .enumerate()
                            .map(|(offset, _)| {
                                tui_components::mention::attachment_label(
                                    tui_components::mention::AttachmentKind::Image,
                                    base + offset,
                                )
                            })
                            .collect();
                        for label in &labels {
                            self.footer.composer.insert_attachment_label(label);
                        }
                        self.staged_images = combined;
                        self.staged_image_labels.extend(labels.clone());
                        self.sync_mention_route();
                        format!(
                            "staged {} image{} ({} total, {} inserted, sent with the next turn)",
                            loaded.len(),
                            if loaded.len() == 1 { "" } else { "s" },
                            self.staged_images.len(),
                            labels.join(" "),
                        )
                    }
                }
            }
        }
    }

    /// Drop staged images without sending them. Placeholders leave the
    /// composer together with their payloads so no orphan label remains.
    pub fn clear_staged_images(&mut self) -> String {
        let labels = std::mem::take(&mut self.staged_image_labels);
        self.staged_images.clear();
        self.footer.composer.remove_attachment_labels(&labels);
        self.sync_mention_route();
        "staged images cleared".to_string()
    }

    /// Drop staged text files without sending them.
    pub fn clear_staged_attachments(&mut self) -> String {
        let labels = std::mem::take(&mut self.staged_attachment_labels);
        self.staged_attachments.clear();
        self.footer.composer.remove_attachment_labels(&labels);
        self.sync_mention_route();
        "staged files cleared".to_string()
    }

    /// Next image placeholder number: one past the current maximum so
    /// deleted middle numbers never collide with fresh stages.
    pub(crate) fn next_image_index(&self) -> usize {
        self.staged_image_labels
            .iter()
            .filter_map(|label| parse_placeholder_index(label, "Image"))
            .max()
            .unwrap_or(self.staged_image_labels.len())
            + 1
    }

    /// Next file placeholder number, same collision rule as images.
    pub(crate) fn next_attachment_index(&self) -> usize {
        self.staged_attachment_labels
            .iter()
            .filter_map(|label| parse_placeholder_index(label, "File"))
            .max()
            .unwrap_or(self.staged_attachment_labels.len())
            + 1
    }

    /// Drop staged payloads whose placeholders no longer occur in the
    /// composer buffer. Called after every edit that can remove a label
    /// so deleting `[Image N]` frees its bytes like the opencode extmark.
    pub fn prune_staged_to_composer(&mut self) {
        let content = self.footer.composer.content().to_string();
        let mut kept_images = Vec::new();
        let mut kept_image_labels = Vec::new();
        for (image, label) in self
            .staged_images
            .iter()
            .cloned()
            .zip(self.staged_image_labels.iter().cloned())
        {
            if content.contains(label.as_str()) {
                kept_images.push(image);
                kept_image_labels.push(label);
            }
        }
        self.staged_images = kept_images;
        self.staged_image_labels = kept_image_labels;
        let mut kept_files = Vec::new();
        let mut kept_file_labels = Vec::new();
        for (file, label) in self
            .staged_attachments
            .iter()
            .cloned()
            .zip(self.staged_attachment_labels.iter().cloned())
        {
            if content.contains(label.as_str()) {
                kept_files.push(file);
                kept_file_labels.push(label);
            }
        }
        self.staged_attachments = kept_files;
        self.staged_attachment_labels = kept_file_labels;
    }

    /// Take staged payloads for a turn, clearing labels alongside so the
    /// next draft starts without orphan placeholders.
    pub fn take_staged_for_turn(
        &mut self,
    ) -> (
        Vec<wf_types::message::UserImageInput>,
        Vec<wf_types::message::UserTextAttachment>,
    ) {
        self.staged_image_labels.clear();
        self.staged_attachment_labels.clear();
        (
            std::mem::take(&mut self.staged_images),
            std::mem::take(&mut self.staged_attachments),
        )
    }

    /// Whether pasted text is solely existing image paths (one per line or
    /// whitespace separated, quotes stripped). Terminal drag-drop lands as
    /// such text; staging it mirrors the opencode image paste branch.
    pub fn paste_image_specs(text: &str) -> Option<Vec<String>> {
        let mut specs = Vec::new();
        for token in text.split_whitespace() {
            let trimmed = token.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
                || trimmed.starts_with("data:")
            {
                specs.push(trimmed.to_string());
                continue;
            }
            let path = std::path::Path::new(trimmed);
            if !path.exists() || !path.is_file() {
                return None;
            }
            wf_types::message::media_type_for_image_path(trimmed)?;
            specs.push(trimmed.to_string());
        }
        (!specs.is_empty()).then_some(specs)
    }

    /// Word at a composer click cell for double-click copy. A mention
    /// returns its whole `@` text and a placeholder returns its label
    /// with a resolvable description so the clipboard never carries an
    /// opaque number alone.
    pub fn composer_word_at_cell(&self, col: u16, row: u16) -> Option<String> {
        let area = self.last_input_area?;
        let byte = self.footer.composer.byte_at_input_cell(area, col, row)?;
        if let Some(semantic) = self.footer.composer.semantic_text_at(byte) {
            if !semantic.trim().is_empty() {
                return Some(self.expand_placeholder_copy(&semantic));
            }
        }
        let (text, display_col) = self
            .footer
            .composer
            .visual_text_at_input_cell(area, col, row)?;
        crate::interactive::mouse_hit::word_at_col(&text, display_col)
    }

    /// Stage text files for the next turn. Loading reuses the headless
    /// attach reader so truncation markers and limits agree; failures
    /// leave the existing stage untouched. Success inserts one visible
    /// `[File N]` placeholder per new entry at the composer cursor.
    pub fn stage_tui_attachments(&mut self, specs: &[String]) -> String {
        let paths: Vec<std::path::PathBuf> = specs.iter().map(std::path::PathBuf::from).collect();
        match wf_cli_shared::turn::load_user_attachment_specs(&paths) {
            Err(err) => format!("cannot stage files: {err}"),
            Ok(loaded) => {
                let mut combined = self.staged_attachments.clone();
                combined.extend(loaded.clone());
                match wf_types::message::validate_user_attachments(&combined) {
                    Err(reason) => {
                        format!("cannot stage files: {reason} (stage unchanged)")
                    }
                    Ok(()) => {
                        let base = self.next_attachment_index();
                        let labels: Vec<String> = loaded
                            .iter()
                            .enumerate()
                            .map(|(offset, _)| {
                                tui_components::mention::attachment_label(
                                    tui_components::mention::AttachmentKind::File,
                                    base + offset,
                                )
                            })
                            .collect();
                        for label in &labels {
                            self.footer.composer.insert_attachment_label(label);
                        }
                        self.staged_attachments = combined;
                        self.staged_attachment_labels.extend(labels.clone());
                        self.sync_mention_route();
                        format!(
                            "staged {} file{} ({} total, {} inserted, sent with the next turn)",
                            loaded.len(),
                            if loaded.len() == 1 { "" } else { "s" },
                            self.staged_attachments.len(),
                            labels.join(" "),
                        )
                    }
                }
            }
        }
    }

    /// Take the pending external-editor request, if any. The shell runs the
    /// handoff outside the event loop's key path.
    pub fn take_pending_editor(&mut self) -> bool {
        let pending = self.pending_editor;
        self.pending_editor = false;
        pending
    }

    /// Tear down the turn task and the interaction poll task.
    pub async fn shutdown(mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = self.poll_task.take() {
            task.abort();
            let _ = task.await;
        }
    }

    pub(crate) fn abort_turn(&mut self) {
        if let Some(task) = self.turn_task.take() {
            task.abort();
        }
        if let Some(task) = self.poll_task.take() {
            task.abort();
        }
    }

    /// Session this controller replays, as its footer reports it.
    pub fn session_id(&self) -> Option<&str> {
        self.footer.state.execution_id.as_deref()
    }

    /// Prefill the composer draft. Branch-from-user-message replays the
    /// selected text so it can be edited and resent in the new tab.
    pub fn prefill_composer(&mut self, text: &str) {
        self.footer.composer.set_text(text);
    }

    /// Snapshot of the session-owned sidebar sections. The shell overlays the
    /// fetched sections (files, git, mcp) before rendering. The context window
    /// is unknown here, so it stays `None` until a model catalog lookup lands.
    pub fn sidebar_snapshot(&self) -> SidebarSnapshot {
        SidebarSnapshot {
            context: ContextSnapshot {
                usage: self.footer.state.usage,
                context_window: None,
                iteration: self.footer.state.iteration,
                message_count: self.footer.state.message_count,
                execution_id: self.footer.state.execution_id.clone(),
                model: self.footer.state.model.clone(),
            },
            todos: self.todos.clone(),
            ..Default::default()
        }
    }

    // Replay-history loading and pagination live in the sibling `replay`
    // module; the performance-policy application lives in the sibling
    // `perf` module.
}
