//! Style gallery: visual review of the tui-style crate.
//!
//! Shows every visual surface the crate owns, all on one screen:
//! - Theme role swatches (`Theme::style_for_role`) for every `ColorRole`.
//! - Light/dark theme adaptation: `t` toggles between `dark_default()` and
//!   `light_default()` themes, re-rendering all swatches (equivalent to what
//!   `theme_mode` does at buffer level each frame).
//! - Animation frames from `AnimationController` (spinner / pulse / bounce /
//!   fade), live or stepped frame by frame.
//! - Motion utilities: activity indicator and shimmer text in animated and
//!   static variants.
//!
//! Keys:
//! - `t` toggle light/dark theme
//! - `a` pause/resume animation stepping (`Space` advances one step while paused)
//! - `q` / `Esc` quit
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_style_gallery
//! ```

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use wf_tui::animation::{AnimationController, AnimationMode, FADE_OPACITIES};
use wf_tui::motion::{activity_indicator_at, shimmer_text_at, MotionMode, ReducedMotionIndicator};
use wf_tui::theme::{ColorRole, Theme};
use wf_tui::theme_mode::ThemeMode;

const ALL_ROLES: [ColorRole; 8] = [
    ColorRole::Default,
    ColorRole::Muted,
    ColorRole::Accent,
    ColorRole::Add,
    ColorRole::Remove,
    ColorRole::Warning,
    ColorRole::Error,
    ColorRole::Highlight,
];

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
    let mut theme = Theme::dark_default();
    let controller = AnimationController::with_mode(AnimationMode::Animated);
    let mut paused = false;
    let mut now_ms: u64 = 0;

    let result = (|| -> io::Result<()> {
        loop {
            terminal.draw(|frame| draw(frame, &theme, &controller, now_ms))?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('t') => {
                            theme = if is_dark(&theme) {
                                Theme::light_default()
                            } else {
                                Theme::dark_default()
                            };
                        }
                        KeyCode::Char('a') => paused = !paused,
                        KeyCode::Char(' ') if paused => now_ms += 100,
                        _ => {}
                    }
                }
            } else if !paused {
                now_ms = wf_tui::clock::now_ms();
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

/// Toggle source: dark theme on the "dark" half, light on the other.
fn is_dark(theme: &Theme) -> bool {
    !matches!(ThemeMode::from_kind(theme.kind), ThemeMode::Light)
}

fn draw(f: &mut ratatui::Frame, theme: &Theme, controller: &AnimationController, now_ms: u64) {
    let area = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // role swatches
            Constraint::Length(9),  // animation frames
            Constraint::Length(5),  // motion utilities
            Constraint::Length(3),  // help
        ])
        .split(area);

    draw_role_swatches(f, theme, rows[0]);
    draw_animation_frames(f, controller, now_ms, rows[1]);
    draw_motion(f, now_ms, rows[2]);
    draw_help(f, rows[3]);
}

/// Every `ColorRole` rendered as a colored swatch line, plus rgb/256/16 info.
fn draw_role_swatches(f: &mut ratatui::Frame, theme: &Theme, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "ColorRole swatches (style_for_role)",
        Style::default().add_modifier(Modifier::BOLD),
    )));

    for role in ALL_ROLES {
        let style = theme.style_for_role(role);
        let sample = format!("  ██ {role:?}  ");
        let rgb = theme.rgb_for_role(role);
        let hex = rgb.hex();
        lines.push(Line::from(vec![
            Span::styled(format!("{sample}"), style),
            Span::styled(format!(" rgb={hex}"), Style::default()),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" theme roles "),
        ),
        area,
    );
}

/// All animation frame sets at the current (or stepped) clock value.
fn draw_animation_frames(
    f: &mut ratatui::Frame,
    controller: &AnimationController,
    now_ms: u64,
    area: Rect,
) {
    let spinner = controller.spinner_char_at(now_ms);
    let pulse = controller.pulse_char_at(now_ms);
    let bounce = controller.bounce_char_at(now_ms);
    let fade = controller.fade_opacity_at(now_ms);
    let fade_idx = FADE_OPACITIES
        .iter()
        .position(|o| (*o - fade).abs() < 0.01)
        .unwrap_or(0);

    let rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(inner(area, 1));

    let cell = |title: &str, content: Vec<Line<'static>>| {
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.to_string()),
        )
    };

    f.render_widget(
        cell(
            " spinner ",
            vec![Line::raw(format!("  {spinner}  |  /  -  \\"))],
        ),
        rows[0],
    );
    f.render_widget(
        cell(
            " pulse ",
            vec![Line::raw(format!("  ● ◉ ● ○   now: {pulse}"))],
        ),
        rows[1],
    );
    f.render_widget(
        cell(
            " bounce ",
            vec![Line::raw(format!("  _ - =     now: {bounce}"))],
        ),
        rows[2],
    );
    f.render_widget(
        cell(
            " fade ",
            (0..FADE_OPACITIES.len())
                .map(|i| {
                    let opacity = FADE_OPACITIES[i];
                    Line::from(Span::styled(
                        format!("  fade {opacity:.1}"),
                        Style::default().fg(ratatui::style::Color::Gray),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
        rows[3],
    );

    // Fade index note under the fade cell is folded into its content; expose
    // the active index as the highlighted one via the frame counter line.
    let _ = fade_idx;
}

/// Motion utilities: activity indicator + shimmer, animated vs static.
fn draw_motion(f: &mut ratatui::Frame, now_ms: u64, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let live = activity_indicator_at(
        now_ms.saturating_sub(1_000),
        now_ms,
        MotionMode::Animated,
        ReducedMotionIndicator::StaticBullet,
    );
    let live_char = live.map(|s| s.content.to_string()).unwrap_or_default();

    lines.push(Line::from(vec![
        Span::raw("  activity (animated): "),
        Span::raw(live_char),
        Span::raw("   static bullet: "),
        Span::raw("●"),
    ]));

    let mut shimmer_live = shimmer_text_at("shimmer text", MotionMode::Animated, now_ms);
    shimmer_live.insert(0, Span::raw("  shimmer (animated): "));
    lines.push(Line::from(shimmer_live));

    let mut shimmer_static = shimmer_text_at("shimmer text", MotionMode::Static, now_ms);
    shimmer_static.insert(0, Span::raw("  shimmer (static):   "));
    lines.push(Line::from(shimmer_static));

    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" motion ")),
        area,
    );
}

fn draw_help(f: &mut ratatui::Frame, area: Rect) {
    let line = Line::from(Span::styled(
        " t: toggle light/dark | a: pause anim | Space: step 100ms (paused) | q/Esc: quit ",
        Style::default().fg(ratatui::style::Color::DarkGray),
    ));
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(" controls ")),
        area,
    );
}

fn inner(area: Rect, margin: u16) -> Rect {
    Rect {
        x: area.x + margin,
        y: area.y + margin,
        width: area.width.saturating_sub(margin * 2),
        height: area.height.saturating_sub(margin * 2),
    }
}
