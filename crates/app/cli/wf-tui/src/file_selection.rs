//! Directory browser modal and async directory scanning.

use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Style, Color};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::keymap::{CKey, Key};
use crate::modal::Modal;
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use crate::theme::Theme;

fn to_color(rgb: crate::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

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

    fn handle_key(&mut self, key: Key) -> crate::modal::ModalAction {
        use crate::modal::ModalAction;
        match key.code {
            CKey::Up | CKey::Char('k') => {
                self.list.navigate(NavigateDir::Prev);
            }
            CKey::Down | CKey::Char('j') => {
                self.list.navigate(NavigateDir::Next);
            }
            CKey::Esc | CKey::Char('q') if !key.ctrl => {
                return ModalAction::Close(crate::modal::ModalResult::Cancelled);
            }
            CKey::Enter => match self.selected_path() {
                Some(path) => {
                    return ModalAction::Close(crate::modal::ModalResult::Value(
                        path.display().to_string(),
                    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_selection_starts_at_the_parent_row() {
        let mut dialog = FileSelectionDialog::new("Pick", "/tmp");
        dialog.set_entries(vec![FileEntry {
            name: "a.txt".into(),
            is_dir: false,
        }]);
        assert_eq!(dialog.selected_name().as_deref(), Some(".."));
        assert_eq!(dialog.selected_path(), None);
        assert_eq!(
            dialog.handle_key(Key::plain(CKey::Down)),
            crate::modal::ModalAction::Stay
        );
        assert_eq!(dialog.selected_path(), Some(PathBuf::from("/tmp/a.txt")));
    }
}
