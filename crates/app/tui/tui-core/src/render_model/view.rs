//! Read-only render view traits.

use tui_style::theme_mode::ThemeMode;

/// Read-only rendering source. Every method has a default so test doubles
/// only override the groups they care about; version accessors are the
/// single invalidation signal for caches.
pub trait RenderView {
    /// Monotonic scrollback content version.
    fn content_version(&self) -> u64 {
        0
    }
    /// Visible streaming prefix text.
    fn streaming_text(&self) -> &str {
        ""
    }
    /// Layout width in columns.
    fn width(&self) -> u16 {
        80
    }
    /// Viewport height in rows.
    fn height(&self) -> u16 {
        24
    }
    /// Whether an overlay, modal or picker is active (forces full frames).
    fn overlay_active(&self) -> bool {
        false
    }
    /// Viewport scroll offset in display rows (0 is tail-follow).
    fn view_scroll(&self) -> usize {
        0
    }
    /// True when automatic tail-follow is paused for reading history.
    fn auto_scroll_paused(&self) -> bool {
        false
    }
    /// Current composer input text.
    fn input_text(&self) -> &str {
        ""
    }
    /// Composer cursor position in characters.
    fn input_cursor(&self) -> usize {
        0
    }
    /// Whether an agent turn is streaming.
    fn is_processing(&self) -> bool {
        false
    }
    /// Queued follow-up prompt count.
    fn queued_count(&self) -> usize {
        0
    }
    /// Active theme mode.
    fn theme_mode(&self) -> ThemeMode {
        ThemeMode::Dark
    }
    /// Discretized animation tick (see [`tui_clock::clock::anim_bucket`]).
    fn anim_tick(&self) -> u64 {
        0
    }
    /// Digest of the footer state.
    fn footer_digest(&self) -> u64 {
        0
    }
    /// Active performance tier marker.
    fn perf_marker(&self) -> &str {
        "perf:full"
    }
    /// Side panel visibility.
    fn side_panel_visible(&self) -> bool {
        false
    }
    /// Diagram pane visibility.
    fn diagram_visible(&self) -> bool {
        false
    }
    /// Image collection signature for cache identity.
    fn image_signature(&self) -> u64 {
        0
    }
    /// Number of committed history rows.
    fn history_count(&self) -> usize {
        0
    }
    /// Plain text of history row `index`, oldest first.
    fn history_line(&self, index: usize) -> Option<&str> {
        let _ = index;
        None
    }
}

/// Per-domain read views converging state access: rendering and key handling
/// depend only on the domains they need, never on a concrete controller.
/// Each trait stays under ten methods by design.
pub trait TranscriptView {
    fn content_version(&self) -> u64;
    fn history_count(&self) -> usize;
    fn history_line(&self, index: usize) -> Option<&str>;
    fn streaming_text(&self) -> &str;
    fn image_signature(&self) -> u64;
}

pub trait InputView {
    fn input_text(&self) -> &str;
    fn input_cursor(&self) -> usize;
    fn is_processing(&self) -> bool;
    fn queued_count(&self) -> usize;
}

pub trait ScrollView {
    fn view_scroll(&self) -> usize;
    fn auto_scroll_paused(&self) -> bool;
}

pub trait LayoutView {
    fn width(&self) -> u16;
    fn height(&self) -> u16;
    fn overlay_active(&self) -> bool;
    fn side_panel_visible(&self) -> bool;
    fn diagram_visible(&self) -> bool;
}

pub trait ThemeView {
    fn theme_mode(&self) -> ThemeMode;
}

pub trait PerfView {
    fn anim_tick(&self) -> u64;
    fn footer_digest(&self) -> u64;
    fn perf_marker(&self) -> &str;
}
