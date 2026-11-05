//! Optional header showcase: disabled (zero rows) plus narrow, medium
//! and wide enabled frames on one screen.
//!
//! Keys: `q`/`Esc` quit.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_header_showcase
//! ```

use std::io;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

use wf_tui::header::{header_height, header_layout, render_header, HeaderConfig, HeaderData};
use wf_tui::theme::Theme;

fn data() -> HeaderData {
    HeaderData {
        version: "1".to_string(),
        model: Some("gpt-4o-mini".to_string()),
        cwd: "/home/user/code/wf-agent/crates/app/tui".to_string(),
        commands: vec![
            "help".to_string(),
            "new".to_string(),
            "compact".to_string(),
            "model".to_string(),
        ],
    }
}

fn main() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide
    )?;
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;
    let theme = Theme::dark_default();
    let _ = &theme;

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(8),
                        Constraint::Length(10),
                        Constraint::Min(0),
                    ])
                    .split(area);
                let widths = [area.width, area.width.min(60), 20];
                let configs = [
                    (HeaderConfig::default(), "disabled: zero rows"),
                    (HeaderConfig::enabled(), "enabled narrow/single"),
                    (HeaderConfig::enabled(), "enabled wide/dual"),
                ];
                for (index, (config, label)) in configs.iter().enumerate() {
                    let width = widths[index].max(1);
                    let height = header_height(*config, width);
                    let lines = render_header(*config, &data(), width);
                    let layout = header_layout(width);
                    let title = format!("{label} width={width} height={height} layout={layout:?}");
                    frame.render_widget(
                        Paragraph::new(lines).block(
                            ratatui::widgets::Block::default()
                                .borders(ratatui::widgets::Borders::ALL)
                                .title(title),
                        ),
                        rows[index],
                    );
                }
            })?;
            if crossterm::event::poll(std::time::Duration::from_millis(100))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.kind == crossterm::event::KeyEventKind::Press
                        && matches!(
                            key.code,
                            crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc
                        )
                    {
                        break;
                    }
                }
            }
        }
        Ok(())
    })();

    drop(terminal);
    crossterm::execute!(
        &mut stdout,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;
    result
}
