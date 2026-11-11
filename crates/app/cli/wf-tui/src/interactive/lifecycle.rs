//! Interactive session lifecycle: start, shutdown and session identity.

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
    /// Prepare an empty session over one server connection. Approvals and
    /// follow-up questions arrive by polling pending interactions while a
    /// turn runs; nothing registers in-process.
    pub async fn start(client: WfClient, execution_id: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut footer = Footer::new();
        footer.state.execution_id = Some(execution_id.clone());
        footer.state.phase = Phase::Idle;
        // Cross-process prompt history: best-effort file tail first; replay
        // user prompts seed older entries later (see `user_prompts_from_lines`).
        if let Some(path) = crate::composer::default_history_path() {
            footer.composer.set_history_file(path);
        }

        let controller = Self {
            client,
            execution_id: execution_id.clone(),
            tx,
            rx,
            reducer: SessionReducer::new(execution_id),
            stream: crate::markdown::MarkdownStream::default(),
            footer,
            scrollback: Vec::new(),
            pending_scroll: Vec::new(),
            content_version: 0,
            prep: PreparedScrollback::new(),
            last_layout_width: 80,
            last_layout_height: 24,
            streaming: None,
            turn_task: None,
            poll_task: None,
            pending_approval: None,
            plugin_dialog: None,
            remembered: ApprovalRemembered::default(),
            scroll_cover: 0,
            tool_started_at: HashMap::new(),
            tool_blocks: HashMap::new(),
            tool_order: Vec::new(),
            subagent_blocks: HashMap::new(),
            subagent_order: Vec::new(),
            subagent_cycle: 0,
            anchors: HashMap::new(),
            anchor_order: Vec::new(),
            anchor_cycle: HashMap::new(),
            anchor_disabled: HashSet::new(),
            tool_config: tui_core::tool_output::ToolOutputConfig::default(),
            activity: Vec::new(),
            todos: Vec::new(),
            viewer_request: None,
            copy_policy: crate::copy::CopyPolicy::disabled(),
            clear_selection_after_copy: false,
            pending_copy: None,
            pending_copy_notice: None,
            exit_tracker: DoublePressTracker::new(SIGINT_DOUBLE_PRESS_WINDOW),
            escape_tracker: DoublePressTracker::new(ESC_DOUBLE_PRESS_WINDOW),
            pager: ReplayPager::default(),
            view_scroll: 0,
            tail_lag: 0,
            scroll_at_top: false,
            show_session_scrollbar: false,
            hit_generation: 0,
            last_badge_rect: None,
            last_scroll_area: None,
            last_input_area: None,
            transcript_selection: None,
            transcript_regions: Vec::new(),
            logical_rows: Vec::new(),
            logical_offsets: Vec::new(),
            logical_total: 0,
            transcript_autoscroll: None,
            hover_cell: None,
            gutter_drag: false,
            graceful: false,
            animation: AnimationController::default_enabled(),
            pacer: SegmentedPacer::default(),
            spinner_enabled: true,
            simplified_render: false,
            last_snapshot: None,
            anim_area: crate::redraw::AnimationArea::default(),
            anim_rows: crate::redraw::AnimRowRecord::new(),
            scroll_cache_key: None,
            last_anim_ms: None,
            history: Vec::new(),
            turn_text: String::new(),
            turn_prompt: String::new(),
            next_seq: 1,
            diagram_requested: 0,
            layout_probe: None,
            image_signature: 0,
            expanded_version: 0,
            panel_profiles: Vec::new(),
            panel_skills: Vec::new(),
            plugin_commands: tui_core::command::CommandRegistry::new(),
            snapshot_commands: Vec::new(),
            local_commands: Vec::new(),
            snapshot_revision: 0,
            local_revision: 0,
            tool_templates: Vec::new(),
            snapshot_tool_templates: Vec::new(),
            local_tool_templates: Vec::new(),
            markdown_rules: Vec::new(),
            snapshot_markdown_rules: Vec::new(),
            local_markdown_rules: Vec::new(),
            setting_fields: Vec::new(),
            snapshot_settings: Vec::new(),
            local_settings: Vec::new(),
            setting_values: std::collections::HashMap::new(),
            plugin_completions: Vec::new(),
            snapshot_completions: Vec::new(),
            local_completions: Vec::new(),
            panel_workflows: Vec::new(),
            panel_auth: Vec::new(),
            panel_mention_files: Vec::new(),
            panel_mention_entities: Vec::new(),
            mention_query_pending: None,
            mention_search_inflight: false,
            dismissed_mention: None,
            prompt_queue: tui_components::queue::PromptQueue::new(),
            active_agent_loop_id: None,
            nested_execution_id: None,
            nested_targets: Vec::new(),
            nested_selected: None,
            append_mode: "followup".to_string(),
            model_override: None,
            session_context: serde_json::Map::new(),
            staged_images: Vec::new(),
            staged_image_labels: Vec::new(),
            staged_attachments: Vec::new(),
            staged_attachment_labels: Vec::new(),
            staged_image_detail: None,
            keymap: tui_core::keymap::Keymap::new(),
            pending_editor: false,
            recap_ticket: 0,
            title_ticket: 0,
            title_fallback: None,
            turn_started_at: None,
            away_this_turn: false,
            focus_visible: None,
            terminal_program_status: tui_terminal::terminal::state::ProgramStatusState::Idle,
            terminal_program_error: None,
            blocker_order: Vec::new(),
            pending_title: None,
        };
        // Rebuild nested targets once at session start so a resumed
        // workflow execution discovers its addressable loops immediately.
        controller.refresh_nested_targets();
        controller
    }

    /// Replace the key resolution table used by the footer panel routes.
    /// The shell injects the user-configured keymap so panel bindings honor
    /// the same overrides as the rest of the interface.
    pub fn set_keymap(&mut self, keymap: tui_core::keymap::Keymap) {
        self.keymap = keymap;
    }

    /// Replace the session-level agent context carried on every turn.
    pub fn set_session_context(&mut self, context: serde_json::Map<String, serde_json::Value>) {
        self.session_context = context;
    }

    /// Current session-level agent context (clone for turn assembly).
    pub fn session_context(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.session_context
    }
}
