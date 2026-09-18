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
//! animation-only scope. Full frames rebuild the scrollback rows and refresh
//! the cache; bottom and animation frames reuse the cached rows and only
//! repaint the footer/input rows and the recorded animation cells.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::interactive::InteractiveController;
use crate::redraw::{decide_scope, AnimationArea, RedrawScope};
use crate::theme::Theme;

impl InteractiveController {
    /// Render the session into the supplied area.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        self.footer.set_now(self.now_ms());

        // Top: scrollback. Middle: footer. Bottom: prompt line.
        let [scroll_area, footer_area, input_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(4),
                Constraint::Length(1),
            ])
            .areas(area);

        let snapshot = self.redraw_snapshot_for(scroll_area.width);
        let scope = decide_scope(self.last_snapshot(), snapshot);
        let scope = self.coerce_scope(scope);
        self.draw_scrollback_scoped(frame, scroll_area, scope);
        self.footer.draw(footer_area, frame.buffer_mut(), theme);
        self.draw_input(frame, input_area);
        self.record_anim_area(scroll_area, footer_area);
        self.set_last_snapshot(snapshot);
        if scope == RedrawScope::AnimationOnly {
            self.last_anim_ms = Some(self.now_ms());
        }
    }

    /// Scope actually honored for this frame: partial scopes require a warm
    /// scrollback cache under the same key, otherwise fall back to full so a
    /// missing seed never leaves stale cells behind.
    fn coerce_scope(&self, scope: RedrawScope) -> RedrawScope {
        if !scope.reuses_scrollback() {
            return scope;
        }
        let key = (self.content_version, self.last_layout_width);
        if self.scroll_cache_key == Some(key) {
            scope
        } else {
            RedrawScope::Full
        }
    }

    fn draw_scrollback_scoped(&mut self, frame: &mut Frame, area: Rect, scope: RedrawScope) {
        let inner = area;

        if self.scrollback.is_empty() && self.streaming.is_none() {
            frame.render_widget(
                Paragraph::new("Type a prompt and press Enter to start an agent turn."),
                inner,
            );
            return;
        }

        let width = inner.width;
        self.last_layout_width = width;
        // Consume the preparation cache: width drift relays out fully while
        // version or length drift is repaired by the fallback path.
        self.prep
            .ensure_for_draw(&self.scrollback, width, self.content_version);
        if scope == RedrawScope::Full
            || self.scroll_cache_key != Some((self.content_version, width))
        {
            self.scroll_rows_cache = self.prep.rows().to_vec();
            self.scroll_cache_key = Some((self.content_version, width));
        }
        // Streaming tail renders every frame by design (it is excluded from
        // the preparation cache); local scopes still skip the scrollback
        // relayout above and only clone the visible window below.
        let now_ms = self.now_ms();
        let mut streaming_lines: Vec<Line<'static>> = Vec::new();
        if let Some(streaming) = &self.streaming {
            let mut rows = if self.simplified_render {
                let mut plain = streaming.clone();
                plain.set_motion_mode(crate::motion::MotionMode::Static);
                plain.display_lines_at(width, now_ms)
            } else {
                streaming.display_lines_at(width, now_ms)
            };
            if self.spinner_enabled {
                // Show spinner animation while streaming; the frame comes from
                // the injected clock so tests assert the exact glyph.
                let spinner_char = self.animation.spinner_char_at(now_ms);
                if let Some(first_line) = rows.first_mut() {
                    // Prepend spinner to the first line
                    let spinner_span = ratatui::text::Span::styled(
                        format!("{} ", spinner_char),
                        ratatui::style::Style::default()
                            .fg(ratatui::style::Color::Cyan)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    );
                    first_line.spans.insert(0, spinner_span);
                }
            }
            streaming_lines = rows;
        }

        // Anchor to the bottom (tail follow) unless the user scrolled up.
        // `view_scroll` counts display rows above the tail; the oldest loaded
        // row is reached when it equals the surplus over the viewport. The
        // scroll pin is refreshed here so key handling (which cannot know the
        // terminal size) can decide whether another page is reachable. Only
        // the visible window is cloned; the cached rows stay shared.
        let capacity = usize::from(inner.height.max(1));
        let total = self.scroll_rows_cache.len() + streaming_lines.len();
        let max_scroll = total.saturating_sub(capacity);
        self.view_scroll = self.view_scroll.min(max_scroll);
        self.scroll_at_top = self.view_scroll >= max_scroll;
        let start = max_scroll - self.view_scroll;
        let end = start.saturating_add(capacity).min(total);
        let cached_len = self.scroll_rows_cache.len();
        let mut visible: Vec<Line<'static>> = Vec::with_capacity(end.saturating_sub(start));
        for index in start..end {
            if index < cached_len {
                visible.push(self.scroll_rows_cache[index].clone());
            } else if let Some(row) = streaming_lines.get(index - cached_len) {
                visible.push(row.clone());
            }
        }
        frame.render_widget(Paragraph::new(visible), inner);
    }

    /// Snapshot variant pinned to the draw width so width drift grades full
    /// before the layout key updates. The animation tick is bucketed so
    /// spinner progress alone grades animation-only and never invalidates
    /// the preparation cache inside a bucket.
    fn redraw_snapshot_for(&self, width: u16) -> crate::redraw::RedrawSnapshot {
        let now_ms = self.now_ms();
        let streaming_text = self.stream.streaming_text();
        crate::redraw::RedrawSnapshot {
            content_version: self.content_version,
            streaming_len: streaming_text.len(),
            streaming_hash: crate::prep_keys::hash_prefix(streaming_text),
            footer_digest: crate::redraw::footer_digest(&self.footer.state),
            width,
            view_scroll: self.view_scroll,
            anim_tick: crate::clock::anim_bucket(now_ms),
            force_full: false,
        }
    }

    /// Record the animation cells an animation-only frame may touch: the
    /// streaming indicator cell and the statusline spinner cell while busy.
    fn record_anim_area(&mut self, scroll_area: Rect, footer_area: Rect) {
        let streaming_cell = self.streaming.as_ref().map(|_| Rect {
            x: scroll_area.x,
            y: scroll_area
                .y
                .saturating_add(scroll_area.height.saturating_sub(1)),
            width: 1,
            height: 1,
        });
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
        self.anim_area = AnimationArea {
            streaming_cell,
            status_cell,
        };
    }

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
        let text = format!("> {}", self.footer.composer.content());
        frame.render_widget(Paragraph::new(text), area);
    }
}
