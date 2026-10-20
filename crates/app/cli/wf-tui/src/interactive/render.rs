//! Presentation layer for the interactive session controller.
//!
//! One `impl` block over [`InteractiveController`] covering the draw
//! responsibility: the full-screen layout (scrollback, footer, input), the
//! scrollback viewport with the streaming tail line and spinner, and the
//! bottom prompt echo. It only reads controller state (aside from the
//! viewport clamp it refreshes each frame); input and event draining live in
//! the sibling modules.
//!
//! Redraw grading: each frame is graded into full, bottom-only or
//! animation-only scope. Full frames refresh the preparation cache key;
//! bottom and animation frames reuse the cached preparation and only
//! repaint the footer/input rows and the recorded animation cells. Only the
//! visible window is ever cloned out of the preparation cache.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::interactive::mouse_hit;
use crate::interactive::InteractiveController;
use crate::redraw::{AnimationArea, RedrawScope};
use crate::theme::Theme;
use tui_components::scrollbar;

/// Columns reserved on the left of the composer input line for the prompt
/// marker (`"> "` on the first row, blanks below).
const PROMPT_PREFIX_WIDTH: u16 = 2;

impl InteractiveController {
    /// Render the session into the supplied area. The single submit point
    /// branches on the shell-provided scope: full rebuilds everything,
    /// bottom-only reuses the cached scrollback preparation and repaints
    /// footer/input, and animation-only touches just the recorded animation
    /// cells. The shell seeds the surface with the previous frame before
    /// calling this, so a partial scope never has to clear the rest.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, requested: RedrawScope) {
        self.footer.set_now(self.now_ms());

        // The bottom is the footer frame plus the composer input line. The
        // footer height follows the active route (panel or blocking view);
        // the input line only exists on the routes that own the composer.
        // The scrollback keeps at least a small viewport.
        let width = area.width.max(1);
        let min_scroll: u16 = 3;
        let bottom_max = area.height.saturating_sub(min_scroll);
        let mut footer_h = self.footer.apply_height();
        let mut input_h = if self.footer.view == crate::footer::FooterView::Prompt
            && self.footer.route.owns_composer_input()
        {
            self.footer.composer_height_for_width(width)
        } else {
            0
        };
        if footer_h.saturating_add(input_h) > bottom_max {
            // The composer line keeps its rows; the panel main area absorbs
            // the rest and the footer clamps its own main area.
            footer_h = bottom_max.saturating_sub(input_h);
            if footer_h < crate::footer::FOOTER_BASE_HEIGHT {
                footer_h = crate::footer::FOOTER_BASE_HEIGHT.min(bottom_max);
                input_h = bottom_max.saturating_sub(footer_h);
            }
        }
        // Record the headless geometry probe for this frame so debug and
        // replay paths can assert layout without inspecting rendered pixels.
        self.layout_probe = Some(crate::render_model::probe_layout(area, footer_h, input_h));
        // Top: scrollback. Middle: footer. Bottom: composer line.
        let [scroll_area, footer_area, input_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(min_scroll),
                Constraint::Length(footer_h),
                Constraint::Length(input_h),
            ])
            .areas(area);

        // The scrollbar switch changes the text width, so the snapshot uses
        // the estimated text width (not the full scroll width). A toggle
        // grades full through the width change with no restart.
        let text_width = self.estimated_text_width(scroll_area);
        let snapshot = self.redraw_snapshot_for(text_width, scroll_area.height);
        let scope = self.coerce_scope(requested);
        match scope {
            RedrawScope::Full => {
                self.draw_scrollback_scoped(frame, scroll_area, scope, theme);
                self.footer.draw(footer_area, frame.buffer_mut(), theme);
                self.draw_input(frame, input_area, theme);
                let badge = self.paint_jump_badge(frame, scroll_area, theme);
                self.record_hit_frame(scroll_area, badge, input_area);
            }
            RedrawScope::BottomOnly => {
                self.draw_scrollback_cached(frame, scroll_area, theme);
                self.footer.draw(footer_area, frame.buffer_mut(), theme);
                self.draw_input(frame, input_area, theme);
                let badge = self.paint_jump_badge(frame, scroll_area, theme);
                self.record_hit_frame(scroll_area, badge, input_area);
            }
            RedrawScope::AnimationOnly => {
                self.draw_anim_cells_only(frame);
            }
            RedrawScope::None => {}
        }
        self.record_anim_area(scroll_area, footer_area);
        self.set_last_snapshot(snapshot);
        if scope == RedrawScope::AnimationOnly {
            self.last_anim_ms = Some(self.now_ms());
        }
    }

    /// Scope actually honored for this frame: partial scopes require a warm
    /// preparation under the same key, otherwise fall back to full so a
    /// missing seed never leaves stale cells behind.
    fn coerce_scope(&self, scope: RedrawScope) -> RedrawScope {
        if !scope.reuses_scrollback() {
            return scope;
        }
        if self.scroll_cache_key == Some(self.prep.key()) {
            scope
        } else {
            RedrawScope::Full
        }
    }

    // Gutter geometry and drag live in the sibling `scrollbar` module; the
    // jump badge lives in the sibling `badge` module.

    /// Text width for the scrollback: full width minus the one-cell gutter
    /// when the bar shows, otherwise the full width.
    fn effective_text_width(full_width: u16, show: bool) -> u16 {
        if show && full_width > 1 {
            full_width - 1
        } else {
            full_width
        }
    }

    /// Estimated total display rows at `width` without touching the cache:
    /// cached rows at their current layout plus the streaming tail rewrapped
    /// at the requested width. Cold caches underestimate by design; the full
    /// path corrects that in the same frame with a second layout. Each
    /// source line yields at least one display row, so the source count is
    /// used as a lower bound to keep cold estimates on the visible side.
    fn estimated_total_rows(&self, width: u16) -> usize {
        let cached = self.prep.total_rows().max(self.scrollback.len());
        let now_ms = self.now_ms();
        let streaming_len = self
            .streaming
            .as_ref()
            .map(|s| s.display_lines_at(width, now_ms).len())
            .unwrap_or(0);
        cached + streaming_len
    }

    /// Estimated text width for snapshots: full width minus the gutter when
    /// the estimate says the bar shows. Empty sessions never show the bar.
    fn estimated_text_width(&self, full_area: Rect) -> u16 {
        if self.scrollback.is_empty() && self.streaming.is_none() {
            return full_area.width;
        }
        let viewport = usize::from(full_area.height);
        let total = self.estimated_total_rows(full_area.width);
        let show = self.scrollbar_visible_for(full_area, total, viewport) && full_area.width > 1;
        Self::effective_text_width(full_area.width, show)
    }

    /// Streaming tail lines rewrapped at `width` with the spinner prefix.
    /// The tail stays out of the preparation cache by design.
    fn streaming_lines_at(&self, width: u16) -> Vec<Line<'static>> {
        let Some(streaming) = &self.streaming else {
            return Vec::new();
        };
        let now_ms = self.now_ms();
        let mut rows = if self.simplified_render {
            let mut plain = streaming.clone();
            plain.set_motion_mode(crate::motion::MotionMode::Static);
            plain.display_lines_at(width, now_ms)
        } else {
            streaming.display_lines_at(width, now_ms)
        };
        if self.spinner_enabled {
            let spinner_char = self.animation.spinner_char_at(now_ms);
            if let Some(first_line) = rows.first_mut() {
                let spinner_span = ratatui::text::Span::styled(
                    format!("{} ", spinner_char),
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                );
                first_line.spans.insert(0, spinner_span);
            }
        }
        rows
    }

    /// Accurate total at the preparation width: cached rows plus the tail
    /// rewrapped at the same width. Used to anchor the badge to the text
    /// area when the bar shows.
    pub(super) fn accurate_total_rows(&self) -> usize {
        let width = self.prep.width();
        let now_ms = self.now_ms();
        let streaming_len = self
            .streaming
            .as_ref()
            .map(|s| s.display_lines_at(width, now_ms).len())
            .unwrap_or(0);
        self.prep.total_rows() + streaming_len
    }

    fn draw_scrollback_scoped(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        _scope: RedrawScope,
        theme: &Theme,
    ) {
        let full_area = area;

        if self.scrollback.is_empty() && self.streaming.is_none() {
            draw_welcome(frame, full_area, theme);
            return;
        }

        // Two-pass width resolution: the estimate (same as the snapshot)
        // seeds the preparation, then the accurate total at that width
        // corrects the one-column edge in the same frame. Steady states do
        // a single layout; only toggle and threshold crossings pay for two.
        let viewport_for_bar = usize::from(full_area.height);
        let capacity = usize::from(full_area.height.max(1));
        let initial_show = self.estimated_text_width(full_area) != full_area.width;
        let mut text_width = Self::effective_text_width(full_area.width, initial_show);
        // Consume the preparation cache: width drift relays out fully while
        // version or length drift is repaired by the fallback path. Only the
        // visible window is cloned below; the cached rows stay shared.
        // Oversize content runs degraded (single entry, see `is_degraded`)
        // but still draws from the same cache with the actual total.
        let _degraded = self.prep.is_degraded();
        self.prep
            .ensure_for_draw(&self.scrollback, text_width, self.content_version);
        let mut streaming_lines = self.streaming_lines_at(text_width);
        let mut cached_len = self.prep.total_rows();
        let mut total = cached_len + streaming_lines.len();
        let mut show_bar =
            self.scrollbar_visible_for(full_area, total, viewport_for_bar) && full_area.width > 1;
        if show_bar != initial_show {
            text_width = Self::effective_text_width(full_area.width, show_bar);
            self.prep
                .ensure_for_draw(&self.scrollback, text_width, self.content_version);
            streaming_lines = self.streaming_lines_at(text_width);
            cached_len = self.prep.total_rows();
            total = cached_len + streaming_lines.len();
            show_bar = self.scrollbar_visible_for(full_area, total, viewport_for_bar)
                && full_area.width > 1;
            text_width = Self::effective_text_width(full_area.width, show_bar);
        }
        self.last_layout_width = text_width;
        self.last_layout_height = full_area.height;
        self.scroll_cache_key = Some(self.prep.key());
        // Streaming tail renders every frame by design (it is excluded from
        // the preparation cache); local scopes reuse the same preparation
        // and only clone the visible window below.

        // Anchor to the bottom (tail follow) unless the user scrolled up.
        // `view_scroll` counts display rows above the tail; the oldest loaded
        // row is reached when it equals the surplus over the viewport. The
        // scroll pin is refreshed here so key handling (which cannot know the
        // terminal size) can decide whether another page is reachable. Only
        // the visible window is cloned; the cached rows stay shared. While
        // tail-following, streaming catch-up slides the window down a few
        // rows per frame instead of teleporting it.
        let max_scroll = total.saturating_sub(capacity);
        self.view_scroll = self.view_scroll.min(max_scroll);
        self.scroll_at_top = self.view_scroll >= max_scroll;
        let effective = self.advance_tail_catchup(capacity);
        let start = scrollbar::top_for_tail(total, capacity, effective);
        let end = start.saturating_add(capacity).min(total);
        // Upgrade deferred code highlights intersecting the window. Geometry
        // matches by construction, so the window stays valid and placeholders
        // never flash.
        self.prep
            .ensure_highlighted_for_window(&self.scrollback, start..end);
        let mut visible: Vec<Line<'static>> = Vec::with_capacity(end.saturating_sub(start));
        if start < cached_len {
            let cached_end = end.min(cached_len);
            visible.extend(
                self.prep
                    .visible_range(start, cached_end - start)
                    .iter()
                    .cloned(),
            );
        }
        for index in start.max(cached_len)..end {
            if let Some(row) = streaming_lines.get(index - cached_len) {
                visible.push(row.clone());
            }
        }
        if show_bar {
            let (content, gutter) = scrollbar::split_content_gutter(full_area);
            frame.render_widget(Paragraph::new(visible), content);
            scrollbar::render(frame, gutter, total, capacity, start, theme);
        } else {
            frame.render_widget(Paragraph::new(visible), full_area);
        }
        self.paint_transcript_selection(frame, full_area, show_bar);
        self.paint_hover_cell(frame, full_area, show_bar);
    }

    /// Snapshot variant pinned to the draw size so width or height drift
    /// grades full before the layout key updates. The animation tick is
    /// bucketed so spinner progress alone grades animation-only and never
    /// invalidates the preparation cache inside a bucket. A pending approval
    /// overlay forces full so the modal never paints over a reused frame.
    fn redraw_snapshot_for(&self, width: u16, height: u16) -> crate::redraw::RedrawSnapshot {
        let now_ms = self.now_ms();
        let streaming_text = self.stream.streaming_text();
        let render_mode = if self.diagram_requested > 0 {
            crate::prep_keys::RENDER_MODE_DIAGRAM
        } else {
            0
        };
        crate::redraw::RedrawSnapshot {
            content_version: self.content_version,
            streaming_len: streaming_text.len(),
            streaming_hash: crate::prep_keys::hash_prefix(streaming_text),
            footer_digest: crate::redraw::footer_digest(&self.footer.state),
            footer_route: self.footer.route.id(),
            input_hash: crate::prep_keys::hash_prefix(self.footer.composer.content()),
            width,
            height,
            render_mode,
            view_scroll: self.view_scroll.saturating_add(self.tail_lag),
            anim_tick: crate::clock::anim_bucket(now_ms),
            image_signature: self.image_signature,
            expanded_version: self.expanded_version,
            aspect_bucket: crate::layout::aspect_bucket(width, height),
            force_full: self.pending_approval.is_some(),
        }
    }

    /// Record the animation cells an animation-only frame may touch: the
    /// streaming indicator cell and the statusline spinner cell while busy.
    /// Stored as loop-held packed rectangles; animation-only frames repaint
    /// exactly these cells. While detached the streaming tail is hidden
    /// behind the jump badge, so its cell stays untouched.
    fn record_anim_area(&mut self, scroll_area: Rect, footer_area: Rect) {
        let streaming_cell = if self.view_scroll > 0 {
            None
        } else {
            self.streaming.as_ref().map(|_| Rect {
                x: scroll_area.x,
                y: scroll_area
                    .y
                    .saturating_add(scroll_area.height.saturating_sub(1)),
                width: 1,
                height: 1,
            })
        };
        let busy = self.footer.state.phase == crate::reducer::Phase::Streaming;
        let status_cell = busy.then(|| {
            let height = self.footer.apply_height().min(footer_area.height);
            Rect {
                x: footer_area.x,
                y: footer_area.y.saturating_add(height.saturating_sub(2)),
                width: 1,
                height: 1,
            }
        });
        self.anim_area = AnimationArea::new(streaming_cell, status_cell);
        self.anim_rows.record(self.anim_area);
    }

    /// Bottom-only path: reuse the cached preparation without relayout and
    /// repaint the visible window. Footer/input are drawn by the caller.
    /// Draws the same gutter as the full path from the same tail conversion.
    fn draw_scrollback_cached(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let full_area = area;
        if self.scrollback.is_empty() && self.streaming.is_none() {
            draw_welcome(frame, full_area, theme);
            return;
        }
        // Steady-state width: the preparation already sits at the text width
        // the full path left behind, so no relayout runs here. The total at
        // that width decides the gutter for this frame.
        let capacity = usize::from(full_area.height.max(1));
        let viewport_for_bar = usize::from(full_area.height);
        let cached_len = self.prep.total_rows();
        let now_ms = self.now_ms();
        let text_width = self.prep.width().max(1);
        let streaming_len = self
            .streaming
            .as_ref()
            .map(|s| s.display_lines_at(text_width, now_ms).len())
            .unwrap_or(0);
        let total = cached_len + streaming_len;
        let show_bar =
            self.scrollbar_visible_for(full_area, total, viewport_for_bar) && full_area.width > 1;
        let text_width = Self::effective_text_width(full_area.width, show_bar);
        self.last_layout_width = text_width;
        self.last_layout_height = full_area.height;
        self.scroll_cache_key = Some(self.prep.key());
        let max_scroll = total.saturating_sub(capacity);
        self.view_scroll = self.view_scroll.min(max_scroll);
        let effective = self.advance_tail_catchup(capacity);
        let start = scrollbar::top_for_tail(total, capacity, effective);
        let end = start.saturating_add(capacity).min(total);
        self.prep
            .ensure_highlighted_for_window(&self.scrollback, start..end);
        let mut visible: Vec<Line<'static>> = Vec::with_capacity(end.saturating_sub(start));
        if start < cached_len {
            let cached_end = end.min(cached_len);
            visible.extend(
                self.prep
                    .visible_range(start, cached_end - start)
                    .iter()
                    .cloned(),
            );
        }
        if let Some(streaming) = &self.streaming {
            let rows = streaming.display_lines_at(text_width, now_ms);
            for index in start.max(cached_len)..end {
                if let Some(row) = rows.get(index - cached_len) {
                    visible.push(row.clone());
                }
            }
        }
        if show_bar {
            let (content, gutter) = scrollbar::split_content_gutter(full_area);
            frame.render_widget(Paragraph::new(visible), content);
            scrollbar::render(frame, gutter, total, capacity, start, theme);
        } else {
            frame.render_widget(Paragraph::new(visible), full_area);
        }
        self.paint_transcript_selection(frame, full_area, show_bar);
    }

    /// Animation-only path: repaint exactly the recorded animation cells.
    /// The scrollback and input keep their previous buffer contents; only
    /// the streaming indicator and spinner cells advance. Falls back to
    /// nothing when no cells were recorded.
    fn draw_anim_cells_only(&mut self, frame: &mut Frame) {
        use ratatui::style::{Color, Modifier, Style};
        if self.anim_rows.is_empty() {
            return;
        }
        let now_ms = self.now_ms();
        let spinner = self.animation.spinner_char_at(now_ms);
        let style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let buf = frame.buffer_mut();
        if let Some(cell) = self.anim_area.streaming_cell() {
            if self.anim_rows.covers(cell.y) {
                buf.set_string(cell.x, cell.y, spinner, style);
            }
        }
        if let Some(cell) = self.anim_area.status_cell() {
            if self.anim_rows.covers(cell.y) {
                buf.set_string(cell.x, cell.y, spinner, style);
            }
        }
    }

    /// Draw the composer input line: the single composer surface of the
    /// session. The panel routes never reach it (they own the footer main
    /// area instead), and the mention route keeps it so the user sees the
    /// query they are typing.
    fn draw_input(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        use ratatui::style::Modifier;
        use ratatui::text::{Line, Span};
        let composer = &self.footer.composer;
        if composer.search_active() {
            let query = composer.content();
            let preview = composer.search_selected_text().unwrap_or_default();
            let count = composer.search_match_count();
            let position = composer.search_position().map(|(sel, _)| sel).unwrap_or(0);
            let mut spans = vec![Span::raw(format!("(reverse-i-search) `{query}': "))];
            if preview.is_empty() {
                spans.push(Span::raw(query.to_string()));
            } else {
                spans.push(Span::raw(preview));
            }
            if count > 0 {
                spans.push(Span::styled(
                    format!(" [{position}/{count}]"),
                    ratatui::style::Style::default().add_modifier(Modifier::DIM),
                ));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), area);
            return;
        }
        if area.height == 0 || area.width <= PROMPT_PREFIX_WIDTH {
            return;
        }
        // The prompt marker column is reserved on the left; the composer
        // renders into the remaining columns, so the text, the placeholder,
        // the mention highlights and the ghost suffix all share one surface.
        let inner = Rect {
            x: area.x + PROMPT_PREFIX_WIDTH,
            y: area.y,
            width: area.width - PROMPT_PREFIX_WIDTH,
            height: area.height,
        };
        let style = crate::footer::theme_style(theme, crate::transcript::Role::Default);
        composer.render(inner, frame.buffer_mut(), style);
        let buf = frame.buffer_mut();
        buf.set_string(area.x, area.y, "> ", style);
        for row in 1..area.height {
            buf.set_string(area.x, area.y + row, "  ", style);
        }
    }

    /// Paint the transcript drag selection as reversed cells over the
    /// scrollback viewport. Runs after the scrollback paragraph so it only
    /// restyles; middle rows cover the text width, edge rows are sliced by
    /// column. The gutter column never highlights.
    fn paint_transcript_selection(&self, frame: &mut Frame, area: Rect, show_bar: bool) {
        let Some(sel) = self.transcript_selection else {
            return;
        };
        if area.width == 0 || area.height == 0 {
            return;
        }
        use ratatui::style::{Modifier, Style};
        let text_width = Self::effective_text_width(area.width, show_bar);
        if text_width == 0 {
            return;
        }
        let (lo_row, hi_row) = sel.row_span();
        let (lo_col, hi_col) = sel.edge_cols();
        let buf = frame.buffer_mut();
        for row in lo_row..=hi_row {
            let y = area.y.saturating_add(row as u16);
            if y >= area.y.saturating_add(area.height) {
                continue;
            }
            let (start, end) = if lo_row == hi_row {
                let (a, b) = if lo_col <= hi_col {
                    (lo_col, hi_col)
                } else {
                    (hi_col, lo_col)
                };
                (a, b)
            } else if row == lo_row {
                (lo_col, usize::MAX)
            } else if row == hi_row {
                (0, hi_col)
            } else {
                (0, usize::MAX)
            };
            let width_cells = usize::from(text_width);
            let start = start.min(width_cells);
            let end = end.min(width_cells);
            if start >= end {
                continue;
            }
            let x = area.x.saturating_add(start as u16);
            let w = (end - start) as u16;
            buf.set_style(
                Rect::new(x, y, w, 1),
                Style::default().add_modifier(Modifier::REVERSED),
            );
        }
    }

    /// Paint the hover affordance for the last motion cell. Full frames
    /// only: hover never requests a frame itself. Clickable content rows
    /// and the jump badge get an underline; everything else stays plain.
    /// Stale cells (outside the viewport, in the gutter, over streaming
    /// rows) simply paint nothing.
    fn paint_hover_cell(&self, frame: &mut Frame, area: Rect, show_bar: bool) {
        use ratatui::style::{Modifier, Style};
        let Some((col, row)) = self.hover_cell else {
            return;
        };
        if area.width == 0 || area.height == 0 {
            return;
        }
        let buf = frame.buffer_mut();
        let underline = Style::default().add_modifier(Modifier::UNDERLINED);
        if let Some(rect) = self.last_badge_rect {
            if mouse_hit::badge_contains(&rect, col, row) {
                buf.set_style(rect, underline);
                return;
            }
        }
        if row < area.y || row >= area.y.saturating_add(area.height) {
            return;
        }
        let text_width = Self::effective_text_width(area.width, show_bar);
        if text_width == 0 {
            return;
        }
        if col < area.x || col >= area.x.saturating_add(text_width) {
            return;
        }
        let display_row = row.saturating_sub(area.y) as usize;
        if display_row >= usize::from(area.height) {
            return;
        }
        if self.content_click_target(display_row).is_none() {
            return;
        }
        buf.set_style(Rect::new(area.x, row, text_width, 1), underline);
    }
}

fn draw_welcome(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    theme: &crate::theme::Theme,
) {
    use ratatui::layout::Alignment;
    use ratatui::style::Modifier;
    use ratatui::text::{Line, Span};

    let title_style = theme
        .bold_style_for_role(crate::theme::ColorRole::Accent)
        .add_modifier(Modifier::BOLD);
    let subtitle_style = theme.dim_style_for_role(crate::theme::ColorRole::Muted);
    let body_style = theme.style_for_role(crate::theme::ColorRole::Default);

    let content: Vec<Line<'static>> = vec![
        Line::from(Span::styled("wf-agent", title_style)),
        Line::from(Span::styled("Modular agent framework", subtitle_style)),
        Line::from(""),
        Line::from("Type a prompt and press Enter to start an agent turn."),
        Line::from("Digit keys open management screens; Ctrl-C twice exits."),
    ];
    let content_height = content.len() as u16;
    let top = area.height.saturating_sub(content_height) / 2;
    let mut lines: Vec<Line<'static>> = (0..top).map(|_| Line::from("")).collect();
    lines.extend(content);

    let paragraph = ratatui::widgets::Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(body_style);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use crate::interactive::InteractiveController;
    use ratatui::layout::Rect;

    #[test]
    fn effective_text_width_reserves_one_gutter_cell() {
        assert_eq!(InteractiveController::effective_text_width(30, true), 29);
        assert_eq!(InteractiveController::effective_text_width(30, false), 30);
        assert_eq!(InteractiveController::effective_text_width(1, true), 1);
        assert_eq!(InteractiveController::effective_text_width(0, false), 0);
    }

    #[tokio::test]
    async fn snapshot_width_uses_text_width() {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        let mut ctl = InteractiveController::start(client, "exec".into()).await;
        for i in 0..50 {
            ctl.scrollback
                .push(crate::transcript::HistoryLine::new(format!("row {i:03}")));
        }
        ctl.content_version = ctl.content_version.wrapping_add(1);
        let full = Rect::new(0, 0, 30, 10);
        ctl.set_session_scrollbar(false);
        assert_eq!(ctl.estimated_text_width(full), 30);
        assert_eq!(
            ctl.redraw_snapshot_for(ctl.estimated_text_width(full), 10)
                .width,
            30
        );
        ctl.set_session_scrollbar(true);
        assert_eq!(ctl.estimated_text_width(full), 29);
        assert_eq!(
            ctl.redraw_snapshot_for(ctl.estimated_text_width(full), 10)
                .width,
            29
        );
    }

    #[tokio::test]
    async fn badge_anchors_to_text_area_when_bar_visible() {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        let mut ctl = InteractiveController::start(client, "exec".into()).await;
        for i in 0..50 {
            ctl.scrollback
                .push(crate::transcript::HistoryLine::new(format!("row {i:03}")));
        }
        ctl.content_version = ctl.content_version.wrapping_add(1);
        ctl.set_session_scrollbar(true);
        ctl.prep
            .ensure_for_draw(&ctl.scrollback, 29, ctl.content_version);
        let full = Rect::new(0, 0, 30, 10);
        assert!(ctl.scrollbar_currently_visible(full));
        let badge_area = Rect::new(full.x, full.y, full.width - 1, full.height);
        assert_eq!(badge_area.width, 29);
        assert_eq!(badge_area.x + badge_area.width, full.x + full.width - 1);
    }

    #[tokio::test]
    async fn render_open_vs_closed_frames_share_text_and_gutter() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let theme = crate::theme::Theme::dark_default();
        let make = |enabled: bool| async move {
            let client =
                wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
            let mut ctl = InteractiveController::start(client, "exec".into()).await;
            for i in 0..50 {
                ctl.scrollback
                    .push(crate::transcript::HistoryLine::new(format!("row {i:03}")));
            }
            ctl.content_version = ctl.content_version.wrapping_add(1);
            ctl.set_session_scrollbar(enabled);
            ctl
        };
        let mut closed = make(false).await;
        let mut open = make(true).await;
        let (closed_buf, open_buf) = {
            let backend = TestBackend::new(30, 15);
            let mut terminal = Terminal::new(backend).expect("backend");
            terminal
                .draw(|frame| {
                    closed.draw(
                        frame,
                        frame.area(),
                        &theme,
                        crate::redraw::RedrawScope::Full,
                    )
                })
                .expect("draw closed");
            let closed_buf = terminal.backend().buffer().clone();
            let backend = TestBackend::new(30, 15);
            let mut terminal = Terminal::new(backend).expect("backend");
            terminal
                .draw(|frame| {
                    open.draw(
                        frame,
                        frame.area(),
                        &theme,
                        crate::redraw::RedrawScope::Full,
                    )
                })
                .expect("draw open");
            let open_buf = terminal.backend().buffer().clone();
            (closed_buf, open_buf)
        };
        let area = ratatui::layout::Rect::new(0, 0, 30, 15);
        let mut gutter_symbols = 0usize;
        for y in 0..area.height {
            for x in 0..area.width - 1 {
                assert_eq!(
                    closed_buf[(x, y)].symbol(),
                    open_buf[(x, y)].symbol(),
                    "text columns must match at ({x},{y})"
                );
            }
            let symbol = open_buf[(area.width - 1, y)].symbol().to_string();
            if symbol == "│" || symbol == "▐" {
                gutter_symbols += 1;
            }
        }
        assert!(
            gutter_symbols > 0,
            "open frame rightmost column must carry track or thumb symbols"
        );
        let closed_right: String = (0..area.height)
            .map(|y| closed_buf[(area.width - 1, y)].symbol().to_string())
            .collect();
        assert!(
            !closed_right.contains('│') && !closed_right.contains('▐'),
            "closed frame must not carry bar symbols: {closed_right:?}"
        );
    }
}
