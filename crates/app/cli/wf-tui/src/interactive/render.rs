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

use crate::interactive::InteractiveController;
use crate::redraw::{decide_scope, AnimationArea, RedrawScope};
use crate::theme::Theme;
use tui_components::scrollbar;

impl InteractiveController {
    /// Render the session into the supplied area. The single submit point
    /// branches on scope: full rebuilds everything, bottom-only reuses the
    /// cached scrollback preparation and repaints footer/input, and
    /// animation-only touches just the recorded animation cells.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.footer.set_now(self.now_ms());

        // Bottom grows with the composer: footer frame plus a mirrored
        // prompt line of the same height. The scrollback keeps at least a
        // small viewport; oversized input is windowed around the cursor.
        let width = area.width.max(1);
        let mut composer_rows = self.footer.composer_height_for_width(width);
        let min_scroll: u16 = 3;
        let available = area.height.saturating_sub(min_scroll);
        let need = crate::footer::FOOTER_BASE_HEIGHT
            .saturating_add(composer_rows)
            .saturating_add(composer_rows);
        if need > available && available > crate::footer::FOOTER_BASE_HEIGHT + 2 {
            let fit = available.saturating_sub(crate::footer::FOOTER_BASE_HEIGHT) / 2;
            composer_rows = fit.max(1).min(composer_rows);
        }
        let footer_h = crate::footer::FOOTER_BASE_HEIGHT.saturating_add(composer_rows);
        let input_h = composer_rows;
        // Top: scrollback. Middle: footer. Bottom: prompt line.
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
        let scope = decide_scope(self.last_snapshot(), snapshot);
        let scope = self.coerce_scope(scope);
        match scope {
            RedrawScope::Full => {
                self.draw_scrollback_scoped(frame, scroll_area, scope, theme);
                self.footer.draw(footer_area, frame.buffer_mut(), theme);
                self.draw_input(frame, input_area);
                self.paint_jump_badge(frame, scroll_area, theme);
            }
            RedrawScope::BottomOnly => {
                self.draw_scrollback_cached(frame, scroll_area, theme);
                self.footer.draw(footer_area, frame.buffer_mut(), theme);
                self.draw_input(frame, input_area);
                self.paint_jump_badge(frame, scroll_area, theme);
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

    /// Whether the session scrollbar is visible for the given window.
    /// Combines the explicit switch with the shared overflow and narrow
    /// guards so hidden frames stay pixel-identical to the no-bar path.
    fn scrollbar_visible_for(&self, full_area: Rect, total: usize, viewport: usize) -> bool {
        self.show_session_scrollbar && scrollbar::visible_in(full_area, total, viewport)
    }

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
    fn accurate_total_rows(&self) -> usize {
        let width = self.prep.width();
        let now_ms = self.now_ms();
        let streaming_len = self
            .streaming
            .as_ref()
            .map(|s| s.display_lines_at(width, now_ms).len())
            .unwrap_or(0);
        self.prep.total_rows() + streaming_len
    }

    /// Whether the gutter is on screen right now, from the accurate total.
    fn scrollbar_currently_visible(&self, full_area: Rect) -> bool {
        if self.scrollback.is_empty() && self.streaming.is_none() {
            return false;
        }
        let viewport = usize::from(full_area.height);
        let total = self.accurate_total_rows();
        self.scrollbar_visible_for(full_area, total, viewport) && full_area.width > 1
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
            frame.render_widget(
                Paragraph::new("Type a prompt and press Enter to start an agent turn."),
                full_area,
            );
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
        // the visible window is cloned; the cached rows stay shared.
        let max_scroll = total.saturating_sub(capacity);
        self.view_scroll = self.view_scroll.min(max_scroll);
        self.scroll_at_top = self.view_scroll >= max_scroll;
        let start = scrollbar::top_for_tail(total, capacity, self.view_scroll);
        let end = start.saturating_add(capacity).min(total);
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
    }

    /// Snapshot variant pinned to the draw size so width or height drift
    /// grades full before the layout key updates. The animation tick is
    /// bucketed so spinner progress alone grades animation-only and never
    /// invalidates the preparation cache inside a bucket. A pending approval
    /// overlay forces full so the modal never paints over a reused frame.
    fn redraw_snapshot_for(&self, width: u16, height: u16) -> crate::redraw::RedrawSnapshot {
        let now_ms = self.now_ms();
        let streaming_text = self.stream.streaming_text();
        crate::redraw::RedrawSnapshot {
            content_version: self.content_version,
            streaming_len: streaming_text.len(),
            streaming_hash: crate::prep_keys::hash_prefix(streaming_text),
            footer_digest: crate::redraw::footer_digest(&self.footer.state),
            width,
            height,
            render_mode: 0,
            view_scroll: self.view_scroll,
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
            frame.render_widget(
                Paragraph::new("Type a prompt and press Enter to start an agent turn."),
                full_area,
            );
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
        let start = scrollbar::top_for_tail(total, capacity, self.view_scroll);
        let end = start.saturating_add(capacity).min(total);
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

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
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
        if area.height == 0 {
            return;
        }
        let available = area.width.saturating_sub(2).max(1);
        let mut rows = composer.visual_text_rows(available);
        if rows.is_empty() {
            rows.push(String::new());
        }
        let cursor_row = composer.cursor_visual_row(available).min(rows.len() - 1);
        let visible = usize::from(area.height);
        let start = if rows.len() <= visible {
            0
        } else {
            (cursor_row + 1)
                .saturating_sub(visible)
                .min(rows.len() - visible)
        };
        let ghost = if composer.is_at_end() {
            composer
                .ghost()
                .map(|suffix| suffix.split('\n').next().unwrap_or("").to_string())
                .filter(|line| !line.is_empty())
        } else {
            None
        };
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (slot, text) in rows.iter().skip(start).take(visible).enumerate() {
            let prefix = if start + slot == 0 { "> " } else { "  " };
            let is_last = start + slot + 1 >= rows.len();
            if is_last {
                if let Some(ref suffix) = ghost {
                    use unicode_width::UnicodeWidthStr;
                    let used = text.width() as u16;
                    let remain = available.saturating_sub(used);
                    let clipped = truncate_echo(suffix, remain);
                    lines.push(Line::from(vec![
                        Span::raw(format!("{prefix}{text}")),
                        Span::styled(
                            clipped,
                            ratatui::style::Style::default().add_modifier(Modifier::DIM),
                        ),
                    ]));
                } else {
                    lines.push(Line::from(vec![Span::raw(format!("{prefix}{text}"))]));
                }
            } else {
                lines.push(Line::from(vec![Span::raw(format!("{prefix}{text}"))]));
            }
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// Whether the detached-history badge belongs on screen.
    fn should_show_jump_badge(&self) -> bool {
        self.view_scroll > 0
    }

    /// Paint the floating jump-to-latest badge over the last scrollback row.
    /// When the gutter shows, the badge anchors to the text area bottom-right
    /// so it never covers the bar; otherwise it uses the full scroll area.
    fn paint_jump_badge(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.should_show_jump_badge() || area.width == 0 || area.height == 0 {
            return;
        }
        let badge_area = if self.scrollbar_currently_visible(area) && area.width > 1 {
            Rect::new(area.x, area.y, area.width - 1, area.height)
        } else {
            area
        };
        let hidden = self.view_scroll;
        let full = jump_badge_text(hidden);
        let short = jump_badge_short_text(hidden);
        let text = pick_jump_badge_text(&full, &short, badge_area.width);
        let Some(text) = text else {
            return;
        };
        use unicode_width::UnicodeWidthStr;
        let text_width = text.width() as u16;
        if text_width >= badge_area.width {
            return;
        }
        let x = badge_area.x.saturating_add(
            badge_area
                .width
                .saturating_sub(text_width)
                .saturating_sub(1),
        );
        let y = badge_area
            .y
            .saturating_add(badge_area.height.saturating_sub(1));
        use ratatui::style::Modifier;
        use tui_style::theme::ColorRole;
        let style = theme
            .style_for_role(ColorRole::Accent)
            .add_modifier(Modifier::BOLD);
        frame.buffer_mut().set_string(x, y, text, style);
    }
}

/// Full badge text carrying the hidden row count and the key hint.
pub(crate) fn jump_badge_text(hidden: usize) -> String {
    format!("↓ {hidden} more · End for latest")
}

/// Compact fallback when the viewport is too narrow for the full hint.
pub(crate) fn jump_badge_short_text(hidden: usize) -> String {
    format!("↓ {hidden} more")
}

/// Choose the full or compact badge text for the available width.
pub(crate) fn pick_jump_badge_text<'a>(
    full: &'a str,
    short: &'a str,
    width: u16,
) -> Option<&'a str> {
    use unicode_width::UnicodeWidthStr;
    if (full.width() as u16) < width {
        Some(full)
    } else if (short.width() as u16) < width {
        Some(short)
    } else {
        None
    }
}

fn truncate_echo(text: &str, width: u16) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let mut out = String::new();
    let mut w = 0u16;
    for grapheme in text.graphemes(true) {
        let gw = grapheme.width() as u16;
        if w + gw > width {
            break;
        }
        out.push_str(grapheme);
        w += gw;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{jump_badge_short_text, jump_badge_text, pick_jump_badge_text};
    use crate::interactive::InteractiveController;
    use ratatui::layout::Rect;
    use tui_components::scrollbar;

    #[test]
    fn badge_text_carries_hidden_count_and_hint() {
        assert_eq!(jump_badge_text(3), "↓ 3 more · End for latest");
        assert_eq!(jump_badge_short_text(3), "↓ 3 more");
    }

    #[test]
    fn badge_text_picks_compact_or_hides_when_narrow() {
        let full = jump_badge_text(12);
        let short = jump_badge_short_text(12);
        assert_eq!(pick_jump_badge_text(&full, &short, 80), Some(full.as_str()));
        assert_eq!(
            pick_jump_badge_text(&full, &short, short.len() as u16 + 1),
            Some(short.as_str())
        );
        assert_eq!(pick_jump_badge_text(&full, &short, 3), None);
    }

    #[test]
    fn effective_text_width_reserves_one_gutter_cell() {
        assert_eq!(InteractiveController::effective_text_width(30, true), 29);
        assert_eq!(InteractiveController::effective_text_width(30, false), 30);
        assert_eq!(InteractiveController::effective_text_width(1, true), 1);
        assert_eq!(InteractiveController::effective_text_width(0, false), 0);
    }

    #[tokio::test]
    async fn scrollbar_switch_and_visibility_combos() {
        let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
        let mut on = InteractiveController::start(client.clone(), "exec".into()).await;
        let mut off = InteractiveController::start(client, "exec".into()).await;
        on.set_session_scrollbar(true);
        off.set_session_scrollbar(false);
        let wide = Rect::new(0, 0, 30, 10);
        let narrow = Rect::new(0, 0, scrollbar::MIN_AREA_WIDTH - 1, 10);
        let tiny = Rect::new(0, 0, 30, 2);
        assert!(on.scrollbar_visible_for(wide, 50, 10));
        assert!(!on.scrollbar_visible_for(wide, 5, 10));
        assert!(!on.scrollbar_visible_for(narrow, 50, 10));
        assert!(!on.scrollbar_visible_for(tiny, 50, 2));
        assert!(!off.scrollbar_visible_for(wide, 50, 10));
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

    #[test]
    fn tail_conversion_includes_streaming_rows() {
        let cached = 90usize;
        let streaming = 10usize;
        let total = cached + streaming;
        let capacity = 10usize;
        assert_eq!(scrollbar::top_for_tail(total, capacity, 0), 90);
        assert_eq!(scrollbar::top_for_tail(total, capacity, 5), 85);
        assert_eq!(scrollbar::top_for_tail(total, capacity, usize::MAX), 0);
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
                .draw(|frame| closed.draw(frame, frame.area(), &theme))
                .expect("draw closed");
            let closed_buf = terminal.backend().buffer().clone();
            let backend = TestBackend::new(30, 15);
            let mut terminal = Terminal::new(backend).expect("backend");
            terminal
                .draw(|frame| open.draw(frame, frame.area(), &theme))
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
