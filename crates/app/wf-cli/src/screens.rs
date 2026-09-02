use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

/// Identifier for the 8 full-TUI screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    Dashboard,
    Workflow,
    Executions,
    Session,
    Checkpoints,
    Search,
    Settings,
    Help,
}

impl ScreenKind {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Workflow => "Workflows",
            Self::Executions => "Executions",
            Self::Session => "Session",
            Self::Checkpoints => "Checkpoints",
            Self::Search => "Search",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }

    pub fn all() -> &'static [ScreenKind] {
        &[
            Self::Dashboard,
            Self::Workflow,
            Self::Executions,
            Self::Session,
            Self::Checkpoints,
            Self::Search,
            Self::Settings,
            Self::Help,
        ]
    }
}

/// One screen entry on the navigation stack.
#[derive(Debug, Clone)]
pub struct Screen {
    pub kind: ScreenKind,
    pub title: String,
}

impl Screen {
    pub fn new(kind: ScreenKind) -> Self {
        let title = kind.title().to_string();
        Self { kind, title }
    }
}

/// Navigation stack managing the 8 TUI screens.
pub struct Screens {
    stack: Vec<Screen>,
    selected: usize,
}

impl Default for Screens {
    fn default() -> Self {
        Self::new()
    }
}

impl Screens {
    pub fn new() -> Self {
        Self {
            stack: vec![Screen::new(ScreenKind::Dashboard)],
            selected: 0,
        }
    }

    pub fn current(&self) -> &Screen {
        self.stack.last().expect("screens stack is never empty")
    }

    pub fn current_kind(&self) -> ScreenKind {
        self.current().kind
    }

    pub fn push(&mut self, kind: ScreenKind) {
        self.stack.push(Screen::new(kind));
        self.selected = 0;
    }

    pub fn pop(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.selected = 0;
            true
        } else {
            false
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn select_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn navigate_to(&mut self, kind: ScreenKind) {
        self.push(kind);
    }

    pub fn go_back(&mut self) -> bool {
        self.pop()
    }

    /// Render the current screen into the frame.
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        match self.current_kind() {
            ScreenKind::Dashboard => Self::draw_dashboard(frame, area, self.selected),
            ScreenKind::Workflow => Self::draw_workflow(frame, area),
            ScreenKind::Executions => Self::draw_executions(frame, area),
            ScreenKind::Session => Self::draw_session(frame, area),
            ScreenKind::Checkpoints => Self::draw_checkpoints(frame, area),
            ScreenKind::Search => Self::draw_search(frame, area),
            ScreenKind::Settings => Self::draw_settings(frame, area),
            ScreenKind::Help => Self::draw_help(frame, area),
        }
    }

    fn draw_dashboard(frame: &mut Frame, area: Rect, selected: usize) {
        let items = [
            "1  Workflows      - asset management",
            "2  Executions     - unified execution tracking",
            "3  Session        - foreground session view",
            "4  Checkpoints    - checkpoint management",
            "5  Search         - cross-resource search",
            "6  Settings       - configuration",
            "7  Dashboard      - (current)",
            "8  Help           - key bindings",
        ];
        let list_items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(idx, text)| {
                let style = if idx == selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(*text).style(style)
            })
            .collect();
        let block = Block::default()
            .title(" Dashboard (q quit, 1-8 switch, ? help) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let list = List::new(list_items).block(block);
        frame.render_widget(list, area);
    }

    fn draw_workflow(frame: &mut Frame, area: Rect) {
        let text = "Workflow Management\n\n- list / show / graph\n- create / update / delete / clone / validate\n- export / import / version\n\nPress Esc to go back";
        let block = Block::default()
            .title(" Workflows ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_executions(frame: &mut Frame, area: Rect) {
        let text = "Executions (unified)\n\n- list / show --timeline --iterations\n- run --workflow <id> [--background]\n- status / pause / resume / cancel / inspect\n- performance / bottleneck / errors / compare / progress\n\nPress Esc to go back";
        let block = Block::default()
            .title(" Executions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_session(frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(area);
        let log_block = Block::default()
            .title(" Session (logs) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));
        let log = Paragraph::new("Session log stream (placeholder)\n\nStreaming agent/workflow events will appear here.\nUse Mini mode (wf --mini) for interactive sessions.")
            .block(log_block);
        frame.render_widget(log, chunks[0]);

        let input_block = Block::default()
            .title(" Input (Enter to send, Esc back) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let input = Paragraph::new(">").block(input_block);
        frame.render_widget(input, chunks[1]);
    }

    fn draw_checkpoints(frame: &mut Frame, area: Rect) {
        let text = "Checkpoints\n\n- checkpoint create <execution-id>\n- checkpoint list <execution-id>\n- checkpoint show <id>\n- checkpoint restore <id> [--resume]\n\nPress Esc to go back";
        let block = Block::default()
            .title(" Checkpoints ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_search(frame: &mut Frame, area: Rect) {
        let text = "Search\n\n- search <query> [--limit]\n- template / workflow / execution search\n\nPress Esc to go back";
        let block = Block::default()
            .title(" Search ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_settings(frame: &mut Frame, area: Rect) {
        let text = "Settings\n\n- storage health / diagnostics\n- llm profiles / templates\n- skill / trigger / tool / script management\n- metrics / analysis configuration\n\nPress Esc to go back";
        let block = Block::default()
            .title(" Settings ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_help(frame: &mut Frame, area: Rect) {
        let text = "Help - Key Bindings\n\n  q / Esc    - quit / back\n  1-8        - switch screens\n  j/k / Up/Down - navigate\n  Enter      - select / push screen\n  ?          - toggle help overlay\n\nScreens: Dashboard, Workflows, Executions, Session, Checkpoints, Search, Settings, Help";
        let block = Block::default()
            .title(" Help ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_navigation_stack() {
        let mut screens = Screens::new();
        assert_eq!(screens.current_kind(), ScreenKind::Dashboard);
        assert_eq!(screens.depth(), 1);
        screens.push(ScreenKind::Workflow);
        assert_eq!(screens.current_kind(), ScreenKind::Workflow);
        assert_eq!(screens.depth(), 2);
        assert!(screens.pop());
        assert_eq!(screens.current_kind(), ScreenKind::Dashboard);
        assert!(!screens.pop());
    }

    #[test]
    fn screens_selection_wraps() {
        let mut screens = Screens::new();
        screens.select_next(3);
        assert_eq!(screens.selected(), 1);
        screens.select_next(3);
        assert_eq!(screens.selected(), 2);
        screens.select_next(3);
        assert_eq!(screens.selected(), 0);
        screens.select_prev(3);
        assert_eq!(screens.selected(), 2);
    }

    #[test]
    fn screen_titles_are_nonempty() {
        for kind in ScreenKind::all() {
            assert!(!kind.title().is_empty());
        }
    }
}
