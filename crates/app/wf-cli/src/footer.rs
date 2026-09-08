//! Bottom pane: the inline split-view stack and status line.
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

use crate::approval_overlay::ApprovalView;
use crate::composer::Composer;
use crate::panels::{
    CommandPalette, MentionPanel, ModelPanel, QueuedPanel, SkillPanel, WorkflowPanel,
};
use crate::question_overlay::QuestionView;
use crate::reducer::Phase;
use crate::status_line::FooterState;
use crate::transcript::Role;
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
    /// Workflow selection panel.
    Workflow,
    /// `@` mention panel (files / skills / workflows).
    Mention,
}

/// The interactive panel attached to the current prompt route. Exactly one
/// panel kind is live per route; opening a route installs its panel and
/// leaving the route drops it.
#[derive(Debug, Clone)]
pub enum PanelState {
    Command(CommandPalette),
    Model(ModelPanel),
    Skill(SkillPanel),
    Queued(QueuedPanel),
    Workflow(WorkflowPanel),
    Mention(MentionPanel),
}

/// The mini footer component. Pure data: rendering and height math only.
#[derive(Debug, Clone)]
pub struct Footer {
    pub view: FooterView,
    pub route: FooterRoute,
    pub state: FooterState,
    pub composer: Composer,
    /// Panel attached to the current prompt route (command palette / model /
    /// skill / queued).
    pub panel: Option<PanelState>,
    /// Pending tool approval (rendered while `FooterView::Permission`).
    pub approval: Option<ApprovalView>,
    /// Pending follow-up question (rendered while `FooterView::Question`).
    pub question: Option<QuestionView>,
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
            panel: None,
            approval: None,
            question: None,
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

    /// Switch the view (permission / question / back to prompt) and reset
    /// the route to the composer.
    pub fn present(&mut self, view: FooterView) {
        self.view = view;
        self.route = FooterRoute::Composer;
        if view == FooterView::Prompt {
            // Returning to the prompt view drops any blocking view state.
            self.approval = None;
            self.question = None;
        }
    }

    /// Switch the route inside the prompt view; the panel of the previous
    /// route is dropped (opening a route installs a fresh panel).
    pub fn set_route(&mut self, route: FooterRoute) {
        self.view = FooterView::Prompt;
        if route != self.route {
            self.panel = None;
        }
        self.route = route;
    }

    /// Close the current panel route and return to the composer.
    pub fn close_panel(&mut self) {
        self.panel = None;
        self.route = FooterRoute::Composer;
    }

    /// Required viewport height for the current view x route.
    pub fn apply_height(&self) -> u16 {
        self.apply_height_with_width(0)
    }

    /// Required viewport height for the current view x route, using the
    /// given terminal width to compute streaming tail row count. Pass 0 to
    /// skip streaming row estimation (the streaming row always occupies at
    /// least 1 row when present).
    pub fn apply_height_with_width(&self, _width: u16) -> u16 {
        let main = match (self.view, self.route) {
            (FooterView::Prompt, FooterRoute::Composer) => COMPOSER_MAIN_HEIGHT,
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
                FooterRoute::Model
                | FooterRoute::Skill
                | FooterRoute::Queued
                | FooterRoute::Workflow
                | FooterRoute::Mention => KeymapContext::Panel,
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
        match (self.view, self.route) {
            (FooterView::Prompt, FooterRoute::Composer) => {
                let style = theme_style(theme, Role::Default);
                self.composer.render(area, buf, style);
            }
            (FooterView::Prompt, _) => {
                let lines = self.panel.as_ref().map(|panel| match panel {
                    PanelState::Command(p) => p.render_lines(area.width, area.height),
                    PanelState::Model(p) => p.render_lines(area.width, area.height),
                    PanelState::Skill(p) => p.render_lines(area.width, area.height),
                    PanelState::Queued(p) => p.render_lines(area.width, area.height),
                    PanelState::Workflow(p) => p.render_lines(area.width, area.height),
                    PanelState::Mention(p) => p.render_lines(area.width, area.height),
                });
                if let Some(lines) = lines {
                    render_rows(area, buf, &lines, theme, Role::Default);
                }
            }
            (FooterView::Permission, _) => {
                if let Some(approval) = &self.approval {
                    let width = usize::from(area.width.max(1));
                    let mut lines: Vec<Line<'static>> = vec![
                        Line::from(Span::raw(approval.title())),
                        Line::from(Span::raw("")),
                    ];
                    // Wrap the arguments preview across the available rows,
                    // reserving the last two rows for the hints block.
                    let preview_rows = area.height.saturating_sub(4) as usize;
                    let preview = approval.arguments_preview(width.saturating_sub(2));
                    let mut remaining = preview_rows;
                    for chunk in wrap_columns(&preview, width.saturating_sub(2)) {
                        if remaining == 0 {
                            break;
                        }
                        lines.push(Line::from(Span::raw(chunk)));
                        remaining -= 1;
                    }
                    lines.push(Line::from(Span::raw("")));
                    lines.push(Line::from(Span::raw(approval.hints())));
                    render_rows(area, buf, &lines, theme, Role::Warning);
                }
            }
            (FooterView::Question, _) => {
                if let Some(question) = &self.question {
                    let lines = question.render_lines(usize::from(area.width.max(1)));
                    render_rows(area, buf, &lines, theme, Role::Accent);
                }
            }
        }
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
            if let Some(usage) = &self.state.usage {
                let total = usage.prompt_tokens + usage.completion_tokens;
                let cost = usage
                    .cost
                    .map(|c| format!(" · ${c:.4}"))
                    .unwrap_or_default();
                spans.push(Span::styled(
                    format!(" ◧ {total} tok{cost}"),
                    theme_style(theme, Role::Muted),
                ));
            }
        }

        render_line_into(area, buf, &Line::from(spans));
    }

    /// The middle status text: agent · model · iteration · message count,
    /// with tool names, the sub-agent count and the last error appended
    /// when present.
    fn status_text(&self) -> String {
        if let Some(err) = &self.state.last_error {
            return format!("⚠ {err}");
        }
        let mut parts = vec![
            format!("wf agent · iter:{}", self.state.iteration),
            format!("msgs:{}", self.state.message_count),
        ];
        if let Some(model) = &self.state.model {
            parts.push(format!("model:{}", model));
        }
        if !self.state.active_tools.is_empty() {
            parts.push(format!("tools:{}", self.state.active_tools.join(", ")));
        }
        if self.state.subagent_count > 0 {
            parts.push(format!("subagents:{}", self.state.subagent_count));
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
pub(crate) fn render_line_into(area: Rect, buf: &mut Buffer, line: &Line<'_>) {
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
pub(crate) fn fill_area(area: Rect, buf: &mut Buffer, ch: char, style: Style) {
    let text: String = ch.to_string().repeat(usize::from(area.width));
    buf.set_string(area.x, area.y, &text, style);
}

/// Render a column of lines into `area` (one row per line, clipped to the
/// area height). Spans without an explicit foreground take `role`'s style.
pub(crate) fn render_rows(area: Rect, buf: &mut Buffer, lines: &[Line<'static>], theme: &Theme, role: Role) {
    let fallback = theme_style(theme, role);
    for (i, line) in lines.iter().enumerate() {
        if i as u16 >= area.height {
            break;
        }
        let styled = Line::from(
            line.spans
                .iter()
                .map(|span| {
                    if span.style.fg.is_none() {
                        Span::styled(span.content.clone(), fallback)
                    } else {
                        span.clone()
                    }
                })
                .collect::<Vec<_>>(),
        );
        let row = Rect {
            x: area.x,
            y: area.y + i as u16,
            width: area.width,
            height: 1,
        };
        render_line_into(row, buf, &styled);
    }
}

/// Split `text` into chunks of at most `width` columns on grapheme
/// boundaries.
pub(crate) fn wrap_columns(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut w = 0usize;
    for g in text.graphemes(true) {
        let gw = g.width();
        if w + gw > width {
            out.push(std::mem::take(&mut current));
            w = 0;
        }
        current.push_str(g);
        w += gw;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Status indicator widget with animation support.
///
/// Displays an animated spinner or static indicator based on the current
/// [`MotionMode`]. Supports different indicator styles and can be used
/// for loading states, processing indicators, or other status displays.
#[derive(Debug, Clone)]
pub struct StatusIndicatorWidget {
    /// Current animation frame index.
    frame: usize,
    /// Motion mode for animation control.
    motion_mode: crate::motion::MotionMode,
    /// Status message to display alongside the indicator.
    message: Option<String>,
    /// Style for the indicator.
    style: Style,
}

impl StatusIndicatorWidget {
    /// Create a new status indicator widget.
    pub fn new() -> Self {
        Self {
            frame: 0,
            motion_mode: crate::motion::MotionMode::default(),
            message: None,
            style: Style::default(),
        }
    }

    /// Create with a specific motion mode.
    pub fn with_motion_mode(mode: crate::motion::MotionMode) -> Self {
        Self {
            frame: 0,
            motion_mode: mode,
            message: None,
            style: Style::default(),
        }
    }

    /// Set the motion mode.
    pub fn set_motion_mode(&mut self, mode: crate::motion::MotionMode) {
        self.motion_mode = mode;
    }

    /// Set the status message.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    /// Clear the status message.
    pub fn clear_message(&mut self) {
        self.message = None;
    }

    /// Set the indicator style.
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Advance the animation frame.
    pub fn tick(&mut self) {
        if self.motion_mode.should_animate() {
            self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
        }
    }

    /// Get the current indicator character.
    pub fn indicator_char(&self) -> char {
        if self.motion_mode.should_animate() {
            SPINNER_FRAMES[self.frame]
        } else {
            // Static bullet for reduced/static mode
            '●'
        }
    }

    /// Render the indicator into a single line.
    pub fn render_line(&self) -> Line<'static> {
        let indicator = self.indicator_char();
        let mut spans = vec![Span::styled(
            format!("{} ", indicator),
            self.style,
        )];

        if let Some(msg) = &self.message {
            spans.push(Span::styled(msg.clone(), self.style));
        }

        Line::from(spans)
    }
}

impl Default for StatusIndicatorWidget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let footer = Footer::new();
        assert_eq!(
            footer.apply_height(),
            FOOTER_BASE_HEIGHT + COMPOSER_MAIN_HEIGHT
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
