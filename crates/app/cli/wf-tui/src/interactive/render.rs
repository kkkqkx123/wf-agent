//! Presentation layer for the interactive session controller.
//!
//! One `impl` block over [`InteractiveController`] covering the draw
//! responsibility: the full-screen layout (scrollback, footer, input), the
//! scrollback viewport with the streaming tail line and spinner, and the
//! bottom prompt echo. It only reads controller state (aside from the
//! viewport clamp it refreshes each frame); input and event draining live in
//! the sibling modules.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::interactive::InteractiveController;
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

        self.draw_scrollback(frame, scroll_area);
        self.footer.draw(footer_area, frame.buffer_mut(), theme);
        self.draw_input(frame, input_area);
    }

    fn draw_scrollback(&mut self, frame: &mut Frame, area: Rect) {
        // No border - Session is now the full-screen primary interface
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
        let mut lines: Vec<Line<'static>> = self.prep.rows().to_vec();
        if let Some(streaming) = &self.streaming {
            // Show spinner animation while streaming
            let spinner_char = self.animation.spinner_char();
            let mut streaming_lines = streaming.display_lines(width);
            if let Some(first_line) = streaming_lines.first_mut() {
                // Prepend spinner to the first line
                let spinner_span = ratatui::text::Span::styled(
                    format!("{} ", spinner_char),
                    ratatui::style::Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                );
                first_line.spans.insert(0, spinner_span);
            }
            lines.extend(streaming_lines);
        }

        // Anchor to the bottom (tail follow) unless the user scrolled up.
        // `view_scroll` counts display rows above the tail; the oldest loaded
        // row is reached when it equals the surplus over the viewport. The
        // scroll pin is refreshed here so key handling (which cannot know the
        // terminal size) can decide whether another page is reachable.
        let capacity = usize::from(inner.height.max(1));
        let max_scroll = lines.len().saturating_sub(capacity);
        self.view_scroll = self.view_scroll.min(max_scroll);
        self.scroll_at_top = self.view_scroll >= max_scroll;
        let start = max_scroll - self.view_scroll;
        let visible: Vec<Line<'static>> = lines.into_iter().skip(start).collect();
        frame.render_widget(Paragraph::new(visible), inner);
    }

    fn draw_input(&self, frame: &mut Frame, area: Rect) {
        let text = format!("> {}", self.footer.composer.content());
        frame.render_widget(Paragraph::new(text), area);
    }
}
