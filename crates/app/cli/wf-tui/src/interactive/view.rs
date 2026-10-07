//! View-layer adapters for the interactive controller: the read-only
//! `render_model` view trait implementations used by headless replay and
//! baseline reports.

use super::InteractiveController;

impl crate::render_model::TranscriptView for InteractiveController {
    fn content_version(&self) -> u64 {
        self.content_version
    }
    fn history_count(&self) -> usize {
        self.scrollback.len()
    }
    fn history_line(&self, _index: usize) -> Option<&str> {
        None
    }
    fn streaming_text(&self) -> &str {
        self.stream.streaming_text()
    }
    fn image_signature(&self) -> u64 {
        self.image_signature
    }
}

impl crate::render_model::InputView for InteractiveController {
    fn input_text(&self) -> &str {
        self.footer.composer.content()
    }
    fn input_cursor(&self) -> usize {
        self.footer.composer.content().len()
    }
    fn is_processing(&self) -> bool {
        self.footer.state.phase == crate::reducer::Phase::Streaming
    }
    fn queued_count(&self) -> usize {
        0
    }
}

impl crate::render_model::ScrollView for InteractiveController {
    fn view_scroll(&self) -> usize {
        self.view_scroll
    }
    fn auto_scroll_paused(&self) -> bool {
        self.view_scroll > 0
    }
}

impl crate::render_model::LayoutView for InteractiveController {
    fn width(&self) -> u16 {
        self.last_layout_width
    }
    fn height(&self) -> u16 {
        self.last_layout_height
    }
    fn overlay_active(&self) -> bool {
        self.approval_reply.is_some()
    }
    fn side_panel_visible(&self) -> bool {
        false
    }
    fn diagram_visible(&self) -> bool {
        self.diagram_requested > 0
    }
}

impl crate::render_model::ThemeView for InteractiveController {
    fn theme_mode(&self) -> tui_style::theme_mode::ThemeMode {
        tui_style::theme_mode::ThemeMode::Dark
    }
}

impl crate::render_model::PerfView for InteractiveController {
    fn anim_tick(&self) -> u64 {
        crate::clock::anim_bucket(self.now_ms())
    }
    fn footer_digest(&self) -> u64 {
        crate::redraw::footer_digest(&self.footer.state)
    }
    fn perf_marker(&self) -> &str {
        "perf:full"
    }
}
