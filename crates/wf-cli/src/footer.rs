//! Mini footer: the inline split-footer view stack and status line.
//!
//! Layout model: the footer owns the bottom
//! `Viewport::Inline(n)` rows and consists of a top decoration row, a main
//! area (composer / panel / permission / question), the status line and a
//! bottom decoration row. [`Footer::apply_height`] derives the required
//! viewport height from the active [`FooterView`] × [`FooterRoute`] pair
//! (base 3 + main area; composer 1, panel 16, permission 12, question 14 —
//! aligned with the opencode `applyHeight` constants); the mini event loop
//! rebuilds the viewport only when the height actually changes.
//!
//! The status line is width responsive (breakpoints 80 / 120): the right
//! summary block only appears at ≥120 columns. A notice replaces the
//! summary region for 3 s and is not overwritten by status updates while
//! active.
//!
//! The component is pure data: `draw` writes into a caller-provided
//! [`Buffer`], `apply_height` is a pure computation and the clock is an
//! injected millisecond value ([`Footer::set_now`]), so everything is
//! unit-testable without a terminal.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::composer::Composer;
use crate::reducer::Phase;
use crate::scrollback::{HistoryLine, Role};
use crate::theme::Theme;

/// Fixed footer frame rows: top decoration + status line + bottom
/// decoration. The main area is added on top of this.
pub const FOOTER_BASE_HEIGHT: u16 = 3;

/// Main-area height constants.
pub const COMPOSER_MAIN_HEIGHT: u16 = 1;
pub const PANEL_MAIN_HEIGHT: u16 = 16;
pub const PERMISSION_MAIN_HEIGHT: u16 = 12;
pub const QUESTION_MAIN_HEIGHT: u16 = 14;

/// Notice lifetime (3 s), aligned with the opencode statusline notice.
pub const NOTICE_TTL_MS: u64 = 3_000;

/// Spinner rotation interval (40 ms).
pub const SPINNER_TICK_MS: u64 = 40;

/// Braille spinner frames.
pub const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Width breakpoints for the status line.
pub const STATUSLINE_SUMMARY_WIDTH: u16 = 120;

/// Columns reserved on the right while a streaming tail is shown: the
/// streamed lines render at `width - 2` so a terminal scrollbar or a
/// resize never makes the streamed tail jump columns mid-stream.
pub const STREAMING_WIDTH_MARGIN: u16 = 2;

/// Which view the footer main area is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FooterView {
    /// Prompt entry (composer or one of the panel routes).
    #[default]
    Prompt,
    /// Tool approval request (blocking; keys go to the approval view).
    Permission,
    /// Follow-up question (blocking; keys go to the question view).
    Question,
}

/// Route inside the prompt view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FooterRoute {
    /// Single-line prompt composer.
    #[default]
    Composer,
    /// `/` command palette.
    Command,
    /// Model selection panel.
    Model,
    /// Skill selection panel.
    Skill,
    /// Queued prompt management panel.
    Queued,
}

/// UI-side footer state: the reducer's [`crate::reducer::FooterState`] plus
/// the mini-only presentation fields (model label, execution id, elapsed
/// time, notice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterState {
    pub phase: Phase,
    pub iteration: u32,
    pub active_tools: Vec<String>,
    pub message_count: u32,
    pub last_error: Option<String>,
    /// Active model profile label (status line).
    pub model: Option<String>,
    /// Active execution id (right summary block / exit hint).
    pub execution_id: Option<String>,
    /// Wall-clock duration of the current turn (ms).
    pub duration_ms: u64,
    /// Pending notice: `(text, expires_at_ms)`.
    pub notice: Option<(String, u64)>,
}

impl Default for FooterState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            iteration: 0,
            active_tools: Vec::new(),
            message_count: 0,
            last_error: None,
            model: None,
            execution_id: None,
            duration_ms: 0,
            notice: None,
        }
    }
}

impl FooterState {
    /// Adopt the reducer footer snapshot (keeps the UI-only fields).
    pub fn merge_reducer(&mut self, reducer: &crate::reducer::FooterState) {
        self.phase = reducer.phase;
        self.iteration = reducer.iteration;
        self.active_tools = reducer.active_tools.clone();
        self.message_count = reducer.message_count;
        self.last_error = reducer.last_error.clone();
    }
}

/// The mini footer component. Pure data: rendering and height math only.
#[derive(Debug, Clone)]
pub struct Footer {
    pub view: FooterView,
    pub route: FooterRoute,
    pub state: FooterState,
    pub composer: Composer,
    /// In-flight streaming line held back from the scrollback (rendered in
    /// the main area until it settles — the "streaming tail line" rule).
    pub streaming: Option<HistoryLine>,
    /// Injected millisecond clock (spinner / notice expiry).
    now_ms: u64,
}

impl Default for Footer {
    fn default() -> Self {
        Self {
            view: FooterView::Prompt,
            route: FooterRoute::Composer,
            state: FooterState::default(),
            composer: Composer::new(),
            streaming: None,
            now_ms: 0,
        }
    }
}

impl Footer {
    /// New footer with the default composer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject the current clock (ms); call before `draw` / `show_notice`.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Switch the view and reset the route to the composer.
    pub fn present(&mut self, view: FooterView) {
        self.view = view;
        self.route = FooterRoute::Composer;
    }

    /// Switch the route inside the prompt view.
    pub fn set_route(&mut self, route: FooterRoute) {
        self.view = FooterView::Prompt;
        self.route = route;
    }

    /// Required viewport height for the current view x route.
    pub fn apply_height(&self) -> u16 {
        self.apply_height_with_width(0)
    }

    /// Required viewport height for the current view x route, using the
    /// given terminal width to compute streaming tail row count. Pass 0 to
    /// skip streaming row estimation (the streaming row always occupies at
    /// least 1 row when present).
    pub fn apply_height_with_width(&self, width: u16) -> u16 {
        let main = match (self.view, self.route) {
            (FooterView::Prompt, FooterRoute::Composer) => {
                let stream_rows = match (&self.streaming, width) {
                    (Some(s), w) if w > 0 => s
                        .desired_height(w.saturating_sub(STREAMING_WIDTH_MARGIN))
                        .max(1),
                    (Some(_), _) => 1,
                    (None, _) => 0,
                };
                COMPOSER_MAIN_HEIGHT + stream_rows
            }
            (FooterView::Prompt, _) => PANEL_MAIN_HEIGHT,
            (FooterView::Permission, _) => PERMISSION_MAIN_HEIGHT,
            (FooterView::Question, _) => QUESTION_MAIN_HEIGHT,
        };
        FOOTER_BASE_HEIGHT + main
    }

    /// The keymap context the footer currently routes keys to.
    pub fn keymap_context(&self) -> crate::keymap::KeymapContext {
        use crate::keymap::KeymapContext;
        match self.view {
            FooterView::Permission => KeymapContext::Approval,
            FooterView::Question => KeymapContext::Question,
            FooterView::Prompt => match self.route {
                FooterRoute::Composer | FooterRoute::Command => KeymapContext::Composer,
                FooterRoute::Model | FooterRoute::Skill | FooterRoute::Queued => {
                    KeymapContext::Panel
                }
            },
        }
    }

    /// Set a notice (replaces any active notice; status updates do not).
    pub fn show_notice(&mut self, text: impl Into<String>) {
        self.state.notice = Some((text.into(), self.now_ms + NOTICE_TTL_MS));
    }

    /// Clear the notice once its TTL has elapsed. Returns whether a repaint
    /// is needed.
    pub fn expire_notice(&mut self) -> bool {
        match self.state.notice {
            Some((_, expires_at)) if self.now_ms >= expires_at => {
                self.state.notice = None;
                true
            }
            _ => false,
        }
    }

    /// Draw the whole footer into `area` of `buf`.
    pub fn draw(&mut self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let height = self.apply_height().min(area.height);
        let area = Rect { height, ..area };
        let [top, main, status, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(height.saturating_sub(3)),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        fill_area(top, buf, '─', theme_style(theme, Role::Muted));
        fill_area(bottom, buf, '─', theme_style(theme, Role::Muted));

        self.draw_main(main, buf, theme);
        self.draw_statusline(status, buf, theme);
    }

    /// Render the main area by the current view x route.
    fn draw_main(&mut self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        if self.view == FooterView::Prompt && self.route == FooterRoute::Composer {
            let style = theme_style(theme, Role::Default);
            if let Some(streaming) = &self.streaming {
                // The streaming tail renders at width - 2 (reserved
                // margin) so its row count stays stable across stream
                // ticks and
                // a resize never shifts the viewport height mid-stream.
                let render_width = area.width.saturating_sub(STREAMING_WIDTH_MARGIN);
                let lines = streaming.display_lines(render_width);
                let stream_rows = lines.len() as u16;
                let [tail, rest] =
                    Layout::vertical([Constraint::Length(stream_rows), Constraint::Min(1)])
                        .areas(area);
                for (i, line) in lines.iter().enumerate() {
                    let row = Rect {
                        x: tail.x,
                        y: tail.y + i as u16,
                        width: tail.width,
                        height: 1,
                    };
                    render_line_into(row, buf, line);
                }
                self.composer.render(rest, buf, style);
            } else {
                self.composer.render(area, buf, style);
            }
        }
        // Panels / approval / question render nothing here; the main
        // area stays quiet (the status line still reports state).
    }

    /// Render the status line: leading icon + label + status text, with a
    /// notice or (≥120 cols) the execution summary on the right.
    fn draw_statusline(&mut self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let busy = self.state.phase == Phase::Streaming;
        let label_style = if busy {
            theme_style(theme, Role::Accent)
        } else {
            theme_style(theme, Role::Muted)
        };

        let mut spans: Vec<Span<'static>> = Vec::new();
        let leading = if busy {
            format!("{} [BUILD] ", spinner_frame(self.now_ms))
        } else {
            "○ [EXIT] ".to_string()
        };
        spans.push(Span::styled(leading, label_style));
        spans.push(Span::raw(self.status_text()));

        if let Some((text, _)) = &self.state.notice {
            spans.push(Span::styled(
                format!(" ⚠ {text}"),
                theme_style(theme, Role::Accent),
            ));
        } else if area.width >= STATUSLINE_SUMMARY_WIDTH {
            if let Some(exec) = &self.state.execution_id {
                spans.push(Span::styled(
                    format!(" ▣ {exec} · {}", format_duration(self.state.duration_ms)),
                    theme_style(theme, Role::Muted),
                ));
            }
        }

        render_line_into(area, buf, &Line::from(spans));
    }

    /// The middle status text: agent · iteration · message count, with tool
    /// names and the last error appended when present.
    fn status_text(&self) -> String {
        if let Some(err) = &self.state.last_error {
            return format!("⚠ {err}");
        }
        let mut parts = vec![
            format!("wf agent · iter:{}", self.state.iteration),
            format!("msgs:{}", self.state.message_count),
        ];
        if !self.state.active_tools.is_empty() {
            parts.push(format!("tools:{}", self.state.active_tools.join(", ")));
        }
        parts.join(" · ")
    }
}

/// Current spinner frame for a clock in ms.
pub fn spinner_frame(now_ms: u64) -> char {
    let index = (now_ms / SPINNER_TICK_MS) as usize % SPINNER_FRAMES.len();
    SPINNER_FRAMES[index]
}

/// Human duration ("2.1s", "500ms").
pub fn format_duration(ms: u64) -> String {
    if ms >= 1_000 {
        let secs = ms as f64 / 1_000.0;
        format!("{secs:.1}s")
    } else {
        format!("{ms}ms")
    }
}

/// Map a scrollback role to a theme style.
pub fn theme_style(theme: &Theme, role: Role) -> Style {
    use ratatui::style::Color;
    let rgb = match role {
        Role::Default => theme.fg,
        Role::Muted => theme.muted,
        Role::Accent => theme.accent,
        Role::Add => theme.add,
        Role::Remove => theme.remove,
        Role::Warning => theme.warning,
        Role::Error => theme.error,
        Role::Highlight => theme.highlight,
    };
    Style::default().fg(Color::Rgb(rgb.r, rgb.g, rgb.b))
}

/// Render one pre-wrapped line into `buf` at `area` (clears the row first,
/// clips graphemes to the area width).
fn render_line_into(area: Rect, buf: &mut Buffer, line: &Line<'_>) {
    let width = usize::from(area.width.max(1));
    buf.set_string(area.x, area.y, " ".repeat(width), Style::default());
    let mut col = 0usize;
    for span in &line.spans {
        if col >= width {
            break;
        }
        let content = span.content.as_ref();
        let avail = width - col;
        let mut take = 0usize;
        let mut take_w = 0usize;
        for g in content.graphemes(true) {
            let gw = g.width();
            if take_w + gw > avail && take > 0 {
                break;
            }
            take_w += gw;
            take += 1;
        }
        let clipped: String = content.graphemes(true).take(take).collect();
        buf.set_string(
            area.x + u16::try_from(col).unwrap_or(u16::MAX),
            area.y,
            &clipped,
            span.style,
        );
        col += clipped.width();
    }
}

/// Fill `area` with a repeated character.
fn fill_area(area: Rect, buf: &mut Buffer, ch: char, style: Style) {
    let text: String = ch.to_string().repeat(usize::from(area.width));
    buf.set_string(area.x, area.y, &text, style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reducer::FooterState as ReducerFooter;

    fn theme() -> Theme {
        Theme::dark_default()
    }

    /// Render a footer into a width×height buffer and return the text.
    fn render_text(footer: &mut Footer, width: u16, height: u16) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| footer.draw(frame.area(), frame.buffer_mut(), &theme()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect()
    }

    #[test]
    fn composer_height_is_base_plus_one() {
        let mut footer = Footer::new();
        assert_eq!(
            footer.apply_height(),
            FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT
        );
        footer.streaming = Some(HistoryLine::new_with_role(
            "…",
            crate::scrollback::LineState::Streaming,
            Role::Muted,
        ));
        assert_eq!(
            footer.apply_height(),
            FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT + 1
        );
        footer.streaming = None;
    }

    #[test]
    fn streaming_rows_reserve_two_columns() {
        // An 80-column row wraps at render width 78 (width - 2) but not at
        // 80: the reserved margin keeps the viewport height stable when the
        // tail streams and reserves space for a terminal scrollbar.
        let mut footer = Footer::new();
        let content = "x".repeat(80);
        footer.streaming = Some(HistoryLine::new_with_role(
            content,
            crate::scrollback::LineState::Streaming,
            Role::Default,
        ));
        assert_eq!(
            footer.apply_height_with_width(80),
            FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT + 2,
            "80 cols stream at 78 => two rows, not one"
        );
        assert_eq!(
            footer.apply_height_with_width(0),
            FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT + 1,
            "unknown width falls back to one streaming row"
        );
        footer.streaming = None;
        assert_eq!(
            footer.apply_height_with_width(80),
            FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT,
            "no streaming tail => base composer height"
        );
    }

    #[test]
    fn panel_permission_question_heights() {
        let mut footer = Footer::new();
        footer.set_route(FooterRoute::Model);
        assert_eq!(
            footer.apply_height(),
            FOOTER_BASE_HEIGHT + PANEL_MAIN_HEIGHT
        );
        footer.present(FooterView::Permission);
        assert_eq!(
            footer.apply_height(),
            FOOTER_BASE_HEIGHT + PERMISSION_MAIN_HEIGHT
        );
        footer.present(FooterView::Question);
        assert_eq!(
            footer.apply_height(),
            FOOTER_BASE_HEIGHT + QUESTION_MAIN_HEIGHT
        );
    }

    #[test]
    fn present_resets_route_to_composer() {
        let mut footer = Footer::new();
        footer.set_route(FooterRoute::Model);
        footer.present(FooterView::Permission);
        assert_eq!(footer.view, FooterView::Permission);
        assert_eq!(footer.route, FooterRoute::Composer);
    }

    #[test]
    fn keymap_context_follows_view_and_route() {
        use crate::keymap::KeymapContext;
        let mut footer = Footer::new();
        assert_eq!(footer.keymap_context(), KeymapContext::Composer);
        footer.set_route(FooterRoute::Skill);
        assert_eq!(footer.keymap_context(), KeymapContext::Panel);
        footer.present(FooterView::Permission);
        assert_eq!(footer.keymap_context(), KeymapContext::Approval);
        footer.present(FooterView::Question);
        assert_eq!(footer.keymap_context(), KeymapContext::Question);
    }

    #[test]
    fn reducer_state_merges_into_ui_state() {
        let mut footer = Footer::new();
        let reducer = ReducerFooter {
            phase: Phase::Streaming,
            iteration: 3,
            active_tools: vec!["bash".to_string()],
            message_count: 7,
            last_error: None,
        };
        footer.state.merge_reducer(&reducer);
        assert_eq!(footer.state.phase, Phase::Streaming);
        assert_eq!(footer.state.iteration, 3);
        assert_eq!(footer.state.active_tools, vec!["bash".to_string()]);
        assert_eq!(footer.state.message_count, 7);
    }

    #[test]
    fn notice_expires_after_the_ttl() {
        let mut footer = Footer::new();
        footer.set_now(1_000);
        footer.show_notice("hello");
        assert!(footer.state.notice.is_some());
        footer.set_now(3_999);
        assert!(!footer.expire_notice(), "still inside the window");
        footer.set_now(4_000);
        assert!(footer.expire_notice(), "ttl elapsed -> repaint");
        assert!(footer.state.notice.is_none());
    }

    #[test]
    fn notice_replaces_previous_notice() {
        let mut footer = Footer::new();
        footer.set_now(1_000);
        footer.show_notice("first");
        footer.set_now(1_500);
        footer.show_notice("second");
        assert_eq!(
            footer.state.notice.as_ref().map(|(t, _)| t.as_str()),
            Some("second")
        );
    }

    #[test]
    fn spinner_frames_rotate_with_the_clock() {
        assert_eq!(spinner_frame(0), '⠋');
        assert_eq!(spinner_frame(SPINNER_TICK_MS), '⠙');
        assert_eq!(
            spinner_frame(SPINNER_TICK_MS * SPINNER_FRAMES.len() as u64),
            '⠋'
        );
    }

    #[test]
    fn duration_formatting() {
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(2_100), "2.1s");
        assert_eq!(format_duration(0), "0ms");
    }

    #[test]
    fn theme_styles_map_every_role() {
        let t = theme();
        for role in [
            Role::Default,
            Role::Muted,
            Role::Accent,
            Role::Add,
            Role::Remove,
            Role::Warning,
            Role::Error,
            Role::Highlight,
        ] {
            let style = theme_style(&t, role);
            assert!(style.fg.is_some(), "role {role:?} has a foreground");
        }
    }

    #[test]
    fn draw_shows_composer_and_exit_statusline() {
        let mut footer = Footer::new();
        let height = footer.apply_height();
        let text = render_text(&mut footer, 80, height);
        assert!(text.contains("> Type a message"), "placeholder: {text}");
        assert!(text.contains("○ [EXIT]"), "idle label: {text}");
        assert!(text.contains("iter:0"), "iteration: {text}");
        assert!(text.contains("msgs:0"), "message count: {text}");
    }

    #[test]
    fn narrow_statusline_hides_the_summary_block() {
        let mut footer = Footer::new();
        footer.state.execution_id = Some("exec-1".to_string());
        footer.state.duration_ms = 2_100;

        let height = footer.apply_height();
        let narrow = render_text(&mut footer, 80, height);
        assert!(!narrow.contains("▣"), "no summary below 120 cols: {narrow}");

        let wide = render_text(&mut footer, 120, height);
        assert!(wide.contains("▣ exec-1"), "summary at ≥120 cols: {wide}");
        assert!(wide.contains("2.1s"), "duration in summary: {wide}");
    }

    #[test]
    fn notice_overrides_the_summary_on_any_width() {
        let mut footer = Footer::new();
        footer.set_now(0);
        footer.show_notice("busy");
        let height = footer.apply_height();
        let text = render_text(&mut footer, 80, height);
        assert!(text.contains("busy"), "notice visible: {text}");
    }
}
