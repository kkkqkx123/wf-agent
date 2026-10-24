//! Panel select showcase: panel-scoped mouse text selection.
//!
//! Two synthetic panels demonstrate the `tui-core` stream selection and the
//! `tui-render` windowed wrap cache on a real `ratatui` terminal: the left
//! panel wraps long prose, the right panel clips preformatted program
//! output. Each panel owns its selection, scroll offset and wrap mode, so
//! dragging never spills across the border. Copying is disabled in the
//! demo; the release payload previews in the status line instead.
//!
//! Synthetic data only, no server involved.
//!
//! Keys:
//! - `Tab` switch focused panel, `Up`/`Down`/`PgUp`/`PgDn` scroll it
//! - `w` toggle focused wrap/clip, `m` toggle continuation marker
//! - mouse drag inside a panel selects, wheel scrolls the hovered panel
//! - `q`/`Esc` quit
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_panel_select_showcase
//! ```

use std::io;
use std::sync::Arc;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Terminal;
use wf_tui::panel_select::{PanelWrapMode, TextPos};
use wf_tui::panel_wrap::PanelWrapCache;

/// One selectable demo panel: owned text plus its wrap cache, scroll
/// offset, logical selection and last drawn text area.
struct DemoPanel {
    title: &'static str,
    text: Arc<str>,
    cache: PanelWrapCache,
    mode: PanelWrapMode,
    marker: bool,
    scroll: u16,
    selection: Option<(TextPos, TextPos)>,
    area: Rect,
}

impl DemoPanel {
    fn new(title: &'static str, body: String, mode: PanelWrapMode) -> Self {
        Self {
            title,
            text: Arc::from(body),
            cache: PanelWrapCache::new(),
            mode,
            marker: false,
            scroll: 0,
            selection: None,
            area: Rect::default(),
        }
    }

    fn refresh(&mut self, width: usize) {
        self.cache
            .set_content(Arc::clone(&self.text), width, self.mode, self.marker);
        self.clamp_scroll();
    }

    fn total_rows(&self) -> u16 {
        self.cache.total_rows().min(usize::from(u16::MAX)) as u16
    }

    fn max_scroll(&self, height: u16) -> u16 {
        self.total_rows().saturating_sub(height)
    }

    fn clamp_scroll(&mut self) {
        let max = self.max_scroll(self.area.height);
        self.scroll = self.scroll.min(max);
    }

    fn begin(&mut self, point: (u16, u16)) {
        if let Some(pos) = self.cache.point_to_textpos(point, self.area, self.scroll) {
            self.selection = Some((pos, pos));
        }
    }

    fn extend(&mut self, point: (u16, u16)) {
        if let Some((anchor, _)) = self.selection {
            if let Some(pos) = self.cache.point_to_textpos(point, self.area, self.scroll) {
                self.selection = Some((anchor, pos));
            }
        }
    }

    fn scroll_by(&mut self, delta: i32) {
        let max = i32::from(self.max_scroll(self.area.height));
        let next = i32::from(self.scroll) + delta;
        self.scroll = next.clamp(0, max) as u16;
    }

    fn selected_text(&self) -> Option<String> {
        let (anchor, cursor) = self.selection?;
        self.cache.extract_text(anchor, cursor, None)
    }
}

/// Whole demo: two panels, a status line and the focused panel index.
struct DemoApp {
    panels: [DemoPanel; 2],
    focus: usize,
    status: String,
}

impl DemoApp {
    fn new() -> Self {
        let prose = (0..60)
            .map(|index| {
                format!(
                    "line {index:02}: the quick brown 狐狸 jumps over prose to exercise wrapping, markers and resize stability"
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let giant = format!("{}\n", "0123456789abcdef ".repeat(1_250));
        let mixed = format!("{prose}\n{giant}tail after the giant single line\n");
        let program = (0..80)
            .map(|index| {
                format!(
                    "{index:04}  TRACE  worker={:<3} span=handle_request micros={:<8} status=ok",
                    index % 8,
                    1000 + index * 37
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            panels: [
                DemoPanel::new("wrap / prose + giant line", mixed, PanelWrapMode::Wrap),
                DemoPanel::new("clip / program output", program, PanelWrapMode::Clip),
            ],
            focus: 0,
            status: String::from("drag inside a panel to select; Tab switches focus"),
        }
    }

    fn hovered(&self, col: u16, row: u16) -> Option<usize> {
        self.panels.iter().position(|panel| {
            panel
                .area
                .contains(ratatui::layout::Position::new(col, row))
        })
    }

    fn preview_selection(&mut self, index: usize) {
        let preview = self.panels[index]
            .selected_text()
            .map(|text| {
                let flat: String = text
                    .chars()
                    .map(|glyph| if glyph == '\n' { '⏎' } else { glyph })
                    .take(72)
                    .collect();
                format!("copy disabled in demo · would copy: {flat}")
            })
            .unwrap_or_else(|| String::from("selection empty"));
        self.status = preview;
    }

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(frame.area());
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);
        for (index, panel) in self.panels.iter_mut().enumerate() {
            let block = Block::default().borders(Borders::ALL).title(format!(
                "{} {}{}",
                panel.title,
                if index == self.focus { "(focused)" } else { "" },
                match panel.mode {
                    PanelWrapMode::Wrap => " [wrap]",
                    PanelWrapMode::Clip => " [clip]",
                }
            ));
            let inner = block.inner(columns[index]);
            panel.area = inner;
            let width = usize::from(inner.width);
            panel.refresh(width);
            panel.clamp_scroll();
            let rows = panel.cache.visible_window(panel.scroll, inner.height);
            let paragraph = Paragraph::new(
                rows.iter()
                    .map(|row| Line::from(row.as_str()))
                    .collect::<Vec<_>>(),
            )
            .block(block);
            frame.render_widget(paragraph, columns[index]);
            if let Some((anchor, cursor)) = panel.selection {
                for (row, from, to) in
                    panel
                        .cache
                        .highlight_cells(anchor, cursor, inner, panel.scroll)
                {
                    let width = to.saturating_sub(from);
                    if width == 0 {
                        continue;
                    }
                    frame.buffer_mut().set_style(
                        Rect::new(from, row, width, 1),
                        Style::default().add_modifier(Modifier::REVERSED),
                    );
                }
            }
            let total = usize::from(panel.total_rows());
            let viewport = usize::from(inner.height);
            if total > viewport && inner.width > 0 {
                let track = Rect::new(
                    inner.x.saturating_add(inner.width).saturating_sub(1),
                    inner.y,
                    1,
                    inner.height,
                );
                let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
                let mut state = ScrollbarState::new(total.saturating_sub(viewport))
                    .position(usize::from(panel.scroll));
                frame.render_stateful_widget(bar, track, &mut state);
            }
        }
        let status = Line::from(self.status.as_str());
        frame.render_widget(Paragraph::new(vec![status]), chunks[1]);
    }
}

fn main() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = DemoApp::new();

    let result = run(&mut terminal, &mut app);
    drop(terminal);

    let _ = crossterm::execute!(
        stdout,
        crossterm::event::DisableMouseCapture,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    );
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<&mut io::Stdout>>,
    app: &mut DemoApp,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Tab => {
                        app.focus = (app.focus + 1) % app.panels.len();
                    }
                    KeyCode::Up => app.panels[app.focus].scroll_by(-1),
                    KeyCode::Down => app.panels[app.focus].scroll_by(1),
                    KeyCode::PageUp => app.panels[app.focus].scroll_by(-10),
                    KeyCode::PageDown => app.panels[app.focus].scroll_by(10),
                    KeyCode::Char('w') => {
                        let panel = &mut app.panels[app.focus];
                        panel.mode = match panel.mode {
                            PanelWrapMode::Wrap => PanelWrapMode::Clip,
                            PanelWrapMode::Clip => PanelWrapMode::Wrap,
                        };
                        panel.selection = None;
                    }
                    KeyCode::Char('m') => {
                        let panel = &mut app.panels[app.focus];
                        panel.marker = !panel.marker;
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(index) = app.hovered(mouse.column, mouse.row) {
                        app.focus = index;
                        app.panels[index].begin((mouse.column, mouse.row));
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    let focus = app.focus;
                    app.panels[focus].extend((mouse.column, mouse.row));
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let focus = app.focus;
                    app.panels[focus].extend((mouse.column, mouse.row));
                    app.preview_selection(focus);
                }
                MouseEventKind::ScrollUp => {
                    if let Some(index) = app.hovered(mouse.column, mouse.row) {
                        app.panels[index].scroll_by(-3);
                    }
                }
                MouseEventKind::ScrollDown => {
                    if let Some(index) = app.hovered(mouse.column, mouse.row) {
                        app.panels[index].scroll_by(3);
                    }
                }
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn drag_selects_confined_text_per_panel() {
        let mut app = DemoApp::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| app.draw(frame))
            .expect("draw succeeds");
        let left = app.panels[0].area;
        app.panels[0].begin((left.x, left.y));
        app.panels[0].extend((left.x + 4, left.y));
        let text = app.panels[0].selected_text().expect("selection");
        assert_eq!(text, "line ");
        let right_before = app.panels[1].selected_text();
        assert_eq!(right_before, None);
    }

    #[test]
    fn resize_keeps_the_same_characters_selected() {
        let mut app = DemoApp::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| app.draw(frame))
            .expect("draw succeeds");
        let left = app.panels[0].area;
        app.panels[0].begin((left.x, left.y));
        app.panels[0].extend((left.x + 4, left.y));
        let before = app.panels[0].selected_text().expect("selection");
        app.panels[0].area = Rect::new(left.x, left.y, left.width / 2, left.height);
        app.panels[0].refresh(usize::from(app.panels[0].area.width));
        let after = app.panels[0].selected_text().expect("selection");
        assert_eq!(before, after);
    }
}
