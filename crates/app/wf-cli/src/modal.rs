//! Modal overlays and the modal stack.
//!
//! A modal is a short-lived overlay that owns the keyboard until it closes
//! with a [`ModalResult`]. Pushing through
//! [`ModalStack::push_with_result`] returns a oneshot receiver: the caller can
//! await the user's answer while the event loop keeps drawing. Dropping the
//! stack (on shutdown) drops the sender, so waiting tasks resolve with
//! `RecvError` instead of hanging.

use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::oneshot;

use crate::keymap::{CKey, Key};
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use crate::theme::{Rgb, Theme};

/// Convert one theme color value into a ratatui color.
fn to_color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Result of a modal interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalResult {
    Confirmed,
    Cancelled,
    Dismissed,
    /// Closed carrying a payload: picked id, typed secret, chosen path.
    Value(String),
}

/// Behavior of a modal component.
pub trait Modal {
    fn title(&self) -> &str;
    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme);
    fn handle_key(&mut self, key: Key) -> ModalAction;
    /// Transparent modals skip the `Clear` pass so the underlying screen keeps
    /// showing through the overlay. Overlays that must fully hide the screen
    /// (confirmations, pickers) stay opaque.
    fn is_transparent(&self) -> bool {
        false
    }
}

/// Action after handling a key in a modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalAction {
    Stay,
    Close(ModalResult),
}

/// One entry on the stack: the modal plus its optional result channel.
struct ModalEntry {
    modal: Box<dyn Modal + Send>,
    result: Option<oneshot::Sender<ModalResult>>,
}

/// A stack of modals with oneshot result channels.
pub struct ModalStack {
    stack: Vec<ModalEntry>,
}

impl Default for ModalStack {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn push(&mut self, modal: Box<dyn Modal + Send>) {
        self.stack.push(ModalEntry {
            modal,
            result: None,
        });
    }

    /// Push a modal and receive the channel its result is delivered on.
    pub fn push_with_result(
        &mut self,
        modal: Box<dyn Modal + Send>,
    ) -> oneshot::Receiver<ModalResult> {
        let (tx, rx) = oneshot::channel();
        self.stack.push(ModalEntry {
            modal,
            result: Some(tx),
        });
        rx
    }

    pub fn pop(&mut self) -> Option<Box<dyn Modal + Send>> {
        self.stack.pop().map(|entry| entry.modal)
    }

    /// Drop every modal, resolving pending waiters with `RecvError` (which
    /// callers map to `Cancelled`).
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn top_mut(&mut self) -> Option<&mut (dyn Modal + Send + '_)> {
        self.stack
            .last_mut()
            .map(|entry| entry.modal.as_mut() as &mut (dyn Modal + Send))
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let Some(top) = self.stack.last() else {
            return;
        };
        if !top.modal.is_transparent() {
            frame.render_widget(Clear, area);
        }
        top.modal.draw(frame, area, theme);
    }

    pub fn handle_key(&mut self, key: Key) -> Option<ModalResult> {
        let top = self.stack.last_mut()?;
        match top.modal.handle_key(key) {
            ModalAction::Stay => None,
            ModalAction::Close(result) => {
                let entry = self.stack.pop().expect("entry present after handling");
                if let Some(tx) = entry.result {
                    let _ = tx.send(result.clone());
                }
                Some(result)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Scroll helpers shared by the read-only viewers
// ---------------------------------------------------------------------------

/// Vertical scroll offset shared by the file and diff viewers.
#[derive(Debug, Clone, Copy, Default)]
struct Scroll {
    offset: usize,
}

impl Scroll {
    fn up(&mut self, step: usize) {
        self.offset = self.offset.saturating_sub(step);
    }

    fn down(&mut self, total: usize, view: usize, step: usize) {
        let max = total.saturating_sub(view);
        self.offset = (self.offset + step).min(max);
    }

    fn home(&mut self) {
        self.offset = 0;
    }

    fn end(&mut self, total: usize, view: usize) {
        self.offset = total.saturating_sub(view);
    }
}

/// Shared `j/k/PageUp/PageDown/Home/End` handling for scrollable viewers.
///
/// `q` and `Esc` fall through to the caller, which decides whether the modal
/// closes.
fn scroll_key(scroll: &mut Scroll, key: Key, total: usize, view: usize) -> bool {
    match key.code {
        CKey::Char('j') | CKey::Down => {
            scroll.down(total, view, 1);
            true
        }
        CKey::Char('k') | CKey::Up => {
            scroll.up(1);
            true
        }
        CKey::PageDown => {
            scroll.down(total, view, view.max(1));
            true
        }
        CKey::PageUp => {
            scroll.up(view.max(1));
            true
        }
        CKey::Home => {
            scroll.home();
            true
        }
        CKey::End => {
            scroll.end(total, view);
            true
        }
        _ => false,
    }
}

/// Render a windowed list of styled rows inside a bordered block.
fn render_viewer(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    rows: &[Line<'static>],
    scroll: &Scroll,
    theme: &Theme,
) {
    let outer = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(to_color(theme.accent)));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view = usize::from(chunks[0].height.max(1));
    let visible: Vec<Line<'static>> = rows
        .iter()
        .skip(scroll.offset)
        .take(view)
        .cloned()
        .collect();
    frame.render_widget(Paragraph::new(visible), chunks[0]);

    let position = if rows.is_empty() {
        "(0/0)".to_string()
    } else {
        format!("({}/{})", scroll.offset + 1, rows.len())
    };
    frame.render_widget(
        Paragraph::new(format!("{position}  {hint}"))
            .style(Style::default().fg(to_color(theme.muted))),
        chunks[1],
    );
}

// ---------------------------------------------------------------------------
// Confirm / help
// ---------------------------------------------------------------------------

/// Simple confirmation modal.
pub struct ConfirmModal {
    title: String,
    message: String,
}

impl ConfirmModal {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

impl Modal for ConfirmModal {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(self.title.clone())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.warning)));
        let paragraph = Paragraph::new(format!("{}\n\n[y] confirm / [n] cancel", self.message))
            .block(block)
            .style(Style::default().fg(to_color(theme.fg)))
            .wrap(Wrap { trim: false });
        let centered = centered_rect(60, 30, area);
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match key.code {
            CKey::Char('y') | CKey::Char('Y') => ModalAction::Close(ModalResult::Confirmed),
            CKey::Char('n') | CKey::Char('N') | CKey::Char('q') | CKey::Char('Q') => {
                ModalAction::Close(ModalResult::Cancelled)
            }
            CKey::Esc => ModalAction::Close(ModalResult::Cancelled),
            _ => ModalAction::Stay,
        }
    }
}

/// Help modal showing key bindings. It is transparent: the screen behind it
/// keeps rendering so the user can read the shortcut in context.
pub struct HelpModal;

impl Modal for HelpModal {
    fn title(&self) -> &str {
        "Help"
    }

    fn is_transparent(&self) -> bool {
        true
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let help_text = "Keys:\n  q / Esc - quit / close\n  1-8 - switch screens\n  ? - help\n  j/k - navigate\n  Enter - select\n  y/n - confirm/cancel\n  Ctrl-Z - suspend (fg to resume)";
        let block = Block::default()
            .title(" Help (?) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.accent)));
        let paragraph = Paragraph::new(help_text)
            .block(block)
            .style(Style::default().fg(to_color(theme.fg)));
        let centered = centered_rect(70, 60, area);
        // Transparent: no Clear, the underlying screen stays visible.
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match key.code {
            CKey::Esc => ModalAction::Close(ModalResult::Dismissed),
            CKey::Char('q') | CKey::Char('?') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

// ---------------------------------------------------------------------------
// File / diff viewers
// ---------------------------------------------------------------------------

/// Read-only text viewer built from pre-wrapped rows.
pub struct FileViewer {
    title: String,
    rows: Vec<Line<'static>>,
    scroll: Scroll,
}

impl FileViewer {
    pub fn new(title: impl Into<String>, content: &str) -> Self {
        let rows = content
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect();
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
        }
    }

    /// Build a viewer from history lines, reusing the scrollback reflow so the
    /// modal wraps exactly like the transcript does.
    pub fn from_history_lines(
        title: impl Into<String>,
        lines: &[crate::scrollback::HistoryLine],
        width: u16,
    ) -> Self {
        let mut rows = Vec::new();
        for line in lines {
            rows.extend(line.display_lines(width));
        }
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

impl Modal for FileViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        render_viewer(
            frame,
            area,
            &self.title,
            "j/k scroll · PgUp/PgDn page · Home/End · q close",
            &self.rows,
            &self.scroll,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        let view = area_rows(20);
        if scroll_key(&mut self.scroll, key, self.rows.len(), view) {
            return ModalAction::Stay;
        }
        match key.code {
            CKey::Esc | CKey::Char('q') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

/// Sign of one diff row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSign {
    Add,
    Remove,
    Context,
    Hunk,
}

impl DiffSign {
    fn style(&self, theme: &Theme) -> Style {
        match self {
            Self::Add => Style::default().fg(to_color(theme.add)),
            Self::Remove => Style::default().fg(to_color(theme.remove)),
            Self::Hunk => Style::default().fg(to_color(theme.accent)),
            Self::Context => Style::default().fg(to_color(theme.fg)),
        }
    }
}

/// One row of a diff: a sign plus its text.
#[derive(Debug, Clone)]
pub struct DiffRow {
    pub sign: DiffSign,
    pub text: String,
}

/// Side-free diff viewer: unified diff in, coloured rows out.
pub struct DiffViewer {
    title: String,
    rows: Vec<DiffRow>,
    scroll: Scroll,
}

impl DiffViewer {
    pub fn new(title: impl Into<String>, rows: Vec<DiffRow>) -> Self {
        Self {
            title: title.into(),
            rows,
            scroll: Scroll::default(),
        }
    }

    /// Parse a unified diff (`+`/`-`/`@@`/context) into coloured rows.
    pub fn from_unified(title: impl Into<String>, diff: &str) -> Self {
        let rows = diff
            .lines()
            .map(|line| {
                let (sign, text) = if let Some(rest) = line.strip_prefix("@@") {
                    (DiffSign::Hunk, format!("@@{rest}"))
                } else if let Some(rest) = line.strip_prefix('+') {
                    (DiffSign::Add, format!("+{rest}"))
                } else if let Some(rest) = line.strip_prefix('-') {
                    (DiffSign::Remove, format!("-{rest}"))
                } else {
                    (DiffSign::Context, line.to_string())
                };
                DiffRow { sign, text }
            })
            .collect();
        Self::new(title, rows)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Theme-coloured display lines for the current diff.
    fn display_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        self.rows
            .iter()
            .map(|row| Line::from(Span::styled(row.text.clone(), row.sign.style(theme))))
            .collect()
    }
}

impl Modal for DiffViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let rows = self.display_lines(theme);
        render_viewer(
            frame,
            area,
            &self.title,
            "j/k scroll · PgUp/PgDn page · Home/End · q close",
            &rows,
            &self.scroll,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        let view = area_rows(20);
        if scroll_key(&mut self.scroll, key, self.rows.len(), view) {
            return ModalAction::Stay;
        }
        match key.code {
            CKey::Esc | CKey::Char('q') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

/// Conservative viewport height used when the real area is unknown.
fn area_rows(default: usize) -> usize {
    default
}

// ---------------------------------------------------------------------------
// Pickers
// ---------------------------------------------------------------------------

/// Shared state for the picker modals: a filterable select list.
#[derive(Debug)]
struct PickerCore {
    list: SelectList<String>,
    filter: String,
}

impl PickerCore {
    fn new(items: Vec<(String, String)>) -> Self {
        let mut group = Group::new(None);
        for (label, data) in items {
            group = group.item(GroupItem::new(label, data));
        }
        Self {
            list: SelectList::groups(vec![group]),
            filter: String::new(),
        }
    }

    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.list.set_filter(None);
        } else {
            self.list.set_filter(Some(&self.filter));
        }
    }

    fn selected_id(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Returns `Some(result)` when the key closes the picker.
    fn handle_key(&mut self, key: Key) -> Option<ModalResult> {
        match key.code {
            CKey::Up | CKey::Char('k') => {
                self.list.navigate(NavigateDir::Prev);
                None
            }
            CKey::Down | CKey::Char('j') => {
                self.list.navigate(NavigateDir::Next);
                None
            }
            CKey::Enter => match self.selected_id() {
                Some(id) => Some(ModalResult::Value(id)),
                None => Some(ModalResult::Cancelled),
            },
            CKey::Esc | CKey::Char('q') if !key.ctrl => Some(ModalResult::Cancelled),
            CKey::Backspace => {
                self.filter.pop();
                self.apply_filter();
                None
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.filter.push(c);
                self.apply_filter();
                None
            }
            _ => None,
        }
    }
}

fn render_picker(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    list: &SelectList<String>,
    filter: &str,
    theme: &Theme,
) {
    let outer = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(to_color(theme.accent)));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("/{filter}")).style(Style::default().fg(to_color(theme.muted))),
        chunks[0],
    );
    let rows = list.render_lines(chunks[1].width, chunks[1].height);
    frame.render_widget(Paragraph::new(rows), chunks[1]);
    frame.render_widget(
        Paragraph::new(format!("{}  {}", list.position_string(), hint))
            .style(Style::default().fg(to_color(theme.muted))),
        chunks[2],
    );
}

/// Model picker: chooses an LLM profile id.
pub struct ModelPicker {
    core: PickerCore,
}

impl ModelPicker {
    /// `models` pairs a display label with the profile id to apply.
    pub fn new(models: Vec<(String, String)>) -> Self {
        Self {
            core: PickerCore::new(models),
        }
    }

    pub fn selected_id(&self) -> Option<String> {
        self.core.selected_id()
    }
}

impl Modal for ModelPicker {
    fn title(&self) -> &str {
        "Select model"
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let centered = centered_rect(70, 70, area);
        render_picker(
            frame,
            centered,
            "Select model",
            "type to filter · j/k move · Enter apply · Esc cancel",
            &self.core.list,
            &self.core.filter,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match self.core.handle_key(key) {
            Some(result) => ModalAction::Close(result),
            None => ModalAction::Stay,
        }
    }
}

/// Session picker: chooses a stored session to replay.
pub struct SessionPicker {
    core: PickerCore,
}

impl SessionPicker {
    /// `sessions` pairs a display label with the session id.
    pub fn new(sessions: Vec<(String, String)>) -> Self {
        Self {
            core: PickerCore::new(sessions),
        }
    }

    pub fn selected_id(&self) -> Option<String> {
        self.core.selected_id()
    }
}

impl Modal for SessionPicker {
    fn title(&self) -> &str {
        "Open session"
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let centered = centered_rect(70, 70, area);
        render_picker(
            frame,
            centered,
            "Open session",
            "type to filter · j/k move · Enter open · Esc cancel",
            &self.core.list,
            &self.core.filter,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match self.core.handle_key(key) {
            Some(result) => ModalAction::Close(result),
            None => ModalAction::Stay,
        }
    }
}

// ---------------------------------------------------------------------------
// Secret input
// ---------------------------------------------------------------------------

/// Single-line secret prompt; the buffer is rendered masked.
pub struct PasswordModal {
    title: String,
    prompt: String,
    input: String,
}

impl PasswordModal {
    pub fn new(title: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            input: String::new(),
        }
    }

    /// The typed secret; empty after a successful submit.
    pub fn value(&self) -> &str {
        &self.input
    }

    fn masked(&self) -> String {
        "*".repeat(self.input.chars().count())
    }
}

impl Modal for PasswordModal {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.error)));
        let body = format!(
            "{}\n\n> {}\n\nEnter submit · Esc cancel",
            self.prompt,
            self.masked()
        );
        let paragraph = Paragraph::new(body).block(block).style(
            Style::default()
                .fg(to_color(theme.fg))
                .add_modifier(Modifier::BOLD),
        );
        let centered = centered_rect(60, 30, area);
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match key.code {
            CKey::Enter => {
                let value = std::mem::take(&mut self.input);
                if value.is_empty() {
                    ModalAction::Close(ModalResult::Cancelled)
                } else {
                    ModalAction::Close(ModalResult::Value(value))
                }
            }
            CKey::Esc => ModalAction::Close(ModalResult::Cancelled),
            CKey::Backspace => {
                self.input.pop();
                ModalAction::Stay
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.input.push(c);
                ModalAction::Stay
            }
            _ => ModalAction::Stay,
        }
    }
}

// ---------------------------------------------------------------------------
// File selection
// ---------------------------------------------------------------------------

/// One directory entry: display name plus whether it is a directory.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Directory browser; the scan runs off the draw path.
pub struct FileSelectionDialog {
    title: String,
    root: PathBuf,
    entries: Vec<FileEntry>,
    list: SelectList<String>,
    filter: String,
}

impl FileSelectionDialog {
    pub fn new(title: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        let mut dialog = Self {
            title: title.into(),
            root: root.into(),
            entries: Vec::new(),
            list: SelectList::groups(vec![Group::new(None)]),
            filter: String::new(),
        };
        dialog.set_entries(Vec::new());
        dialog
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Replace the listed entries and rebuild the select list.
    pub fn set_entries(&mut self, entries: Vec<FileEntry>) {
        self.entries = entries;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let mut group = Group::new(None).item(GroupItem::new("..".to_string(), "..".to_string()));
        for entry in &self.entries {
            let label = if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            group = group.item(GroupItem::new(label, entry.name.clone()));
        }
        self.list = SelectList::groups(vec![group]);
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.list.set_filter(None);
        } else {
            self.list.set_filter(Some(&self.filter));
        }
    }

    pub fn selected_name(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Absolute path of the current selection; `None` for the `..` row.
    pub fn selected_path(&self) -> Option<PathBuf> {
        let name = self.selected_name()?;
        if name == ".." {
            None
        } else {
            Some(self.root.join(name))
        }
    }
}

impl Modal for FileSelectionDialog {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let outer = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.accent)));
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

        frame.render_widget(
            Paragraph::new(format!("{}  /{}", self.root.display(), self.filter))
                .style(Style::default().fg(to_color(theme.muted))),
            chunks[0],
        );
        let rows = self.list.render_lines(chunks[1].width, chunks[1].height);
        frame.render_widget(Paragraph::new(rows), chunks[1]);
        frame.render_widget(
            Paragraph::new("type to filter · j/k move · Enter open · Esc cancel")
                .style(Style::default().fg(to_color(theme.muted))),
            chunks[2],
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match key.code {
            CKey::Up | CKey::Char('k') => {
                self.list.navigate(NavigateDir::Prev);
            }
            CKey::Down | CKey::Char('j') => {
                self.list.navigate(NavigateDir::Next);
            }
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                return ModalAction::Close(ModalResult::Cancelled);
            }
            CKey::Enter => match self.selected_path() {
                Some(path) => {
                    return ModalAction::Close(ModalResult::Value(path.display().to_string()));
                }
                None => {
                    // `..` moves up one directory in place.
                    if let Some(parent) = self.root.parent() {
                        self.root = parent.to_path_buf();
                        self.entries.clear();
                        self.rebuild();
                    }
                }
            },
            CKey::Backspace => {
                self.filter.pop();
                self.apply_filter();
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.filter.push(c);
                self.apply_filter();
            }
            _ => {}
        }
        ModalAction::Stay
    }
}

/// Scan a directory off the draw path; directories are listed first.
pub async fn scan_dir(root: PathBuf) -> std::io::Result<Vec<FileEntry>> {
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir,
            });
        }
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        Ok(entries)
    })
    .await
    .unwrap_or_else(|err| Err(std::io::Error::other(err)))
}

/// Center a rectangle covering `percent_x` x `percent_y` of `r`.
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_stack_push_pop() {
        let mut stack = ModalStack::new();
        assert!(stack.is_empty());
        stack.push(Box::new(ConfirmModal::new("t", "m")));
        assert_eq!(stack.len(), 1);
        stack.push(Box::new(HelpModal));
        assert_eq!(stack.len(), 2);
        let _ = stack.pop();
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn confirm_modal_key_handling() {
        let mut modal = ConfirmModal::new("Delete?", "Sure?");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Char('y'))),
            ModalAction::Close(ModalResult::Confirmed)
        );
        let mut modal = ConfirmModal::new("Delete?", "Sure?");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Char('n'))),
            ModalAction::Close(ModalResult::Cancelled)
        );
    }

    #[tokio::test]
    async fn push_with_result_delivers_the_answer() {
        let mut stack = ModalStack::new();
        let rx = stack.push_with_result(Box::new(ConfirmModal::new("Delete?", "Sure?")));
        assert_eq!(stack.len(), 1);
        assert_eq!(
            stack.handle_key(Key::plain(CKey::Char('y'))),
            Some(ModalResult::Confirmed)
        );
        assert!(stack.is_empty());
        assert_eq!(rx.await, Ok(ModalResult::Confirmed));
    }

    #[tokio::test]
    async fn clearing_the_stack_cancels_waiters() {
        let mut stack = ModalStack::new();
        let rx = stack.push_with_result(Box::new(ConfirmModal::new("Delete?", "Sure?")));
        stack.clear();
        assert!(rx.await.is_err(), "dropped sender must resolve the waiter");
    }

    #[test]
    fn help_modal_is_transparent_and_confirm_is_not() {
        assert!(HelpModal.is_transparent());
        assert!(!ConfirmModal::new("t", "m").is_transparent());
    }

    #[test]
    fn diff_viewer_parses_unified_diff() {
        let viewer =
            DiffViewer::from_unified("diff", "@@ -1,2 +1,2 @@\n context\n-removed\n+added\n");
        assert_eq!(viewer.row_count(), 4);
        let mut viewer = viewer;
        assert_eq!(
            viewer.handle_key(Key::plain(CKey::Char('q'))),
            ModalAction::Close(ModalResult::Dismissed)
        );
    }

    #[test]
    fn file_viewer_splits_content_into_rows() {
        let viewer = FileViewer::new("readme", "one\ntwo\nthree");
        assert_eq!(viewer.row_count(), 3);
    }

    #[test]
    fn model_picker_returns_the_selected_id() {
        let models = vec![
            ("gpt-4o · openai".to_string(), "openai:gpt-4o".to_string()),
            (
                "claude · anthropic".to_string(),
                "anthropic:claude".to_string(),
            ),
        ];
        let mut picker = ModelPicker::new(models);
        assert_eq!(picker.selected_id().as_deref(), Some("openai:gpt-4o"));
        assert_eq!(picker.handle_key(Key::plain(CKey::Down)), ModalAction::Stay);
        assert_eq!(picker.selected_id().as_deref(), Some("anthropic:claude"));
        assert_eq!(
            picker.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("anthropic:claude".to_string()))
        );
    }

    #[test]
    fn session_picker_filters_and_selects() {
        let sessions = vec![
            ("today · fix build".to_string(), "sess-1".to_string()),
            ("yesterday · refactor".to_string(), "sess-2".to_string()),
        ];
        let mut picker = SessionPicker::new(sessions);
        let _ = picker.handle_key(Key::plain(CKey::Char('y')));
        assert_eq!(picker.core.filter, "y");
        assert_eq!(
            picker.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("sess-2".to_string()))
        );
    }

    #[test]
    fn password_modal_masks_and_submits() {
        let mut modal = PasswordModal::new("Token", "Enter API token");
        let _ = modal.handle_key(Key::plain(CKey::Char('a')));
        let _ = modal.handle_key(Key::plain(CKey::Char('b')));
        assert_eq!(modal.masked(), "**");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("ab".to_string()))
        );
    }

    #[test]
    fn file_selection_starts_at_the_parent_row() {
        let mut dialog = FileSelectionDialog::new("Pick", "/tmp");
        dialog.set_entries(vec![FileEntry {
            name: "a.txt".into(),
            is_dir: false,
        }]);
        assert_eq!(dialog.selected_name().as_deref(), Some(".."));
        assert_eq!(dialog.selected_path(), None);
        assert_eq!(dialog.handle_key(Key::plain(CKey::Down)), ModalAction::Stay);
        assert_eq!(dialog.selected_path(), Some(PathBuf::from("/tmp/a.txt")));
    }
}
