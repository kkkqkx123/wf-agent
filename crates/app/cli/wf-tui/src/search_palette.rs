//! Search palette overlay state: input draft plus last results.
//!
//! The palette is transient and never enters the navigation stack. It owns
//! its input and results so any screen, including the session screen, can
//! open it without leaving the current page.

use crate::screens::SearchRow;

/// Search palette state.
#[derive(Debug, Default)]
pub struct SearchPalette {
    input: String,
    query: String,
    results: Vec<SearchRow>,
    total: usize,
    truncated: bool,
    running: bool,
    selected: usize,
    error: Option<String>,
}

impl SearchPalette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn results(&self) -> &[SearchRow] {
        &self.results
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
    }

    pub fn pop(&mut self) {
        self.input.pop();
        self.selected = 0;
    }

    pub fn append_paste(&mut self, text: &str) {
        if !text.is_empty() {
            self.input.push_str(text);
            self.selected = 0;
        }
    }

    /// Take the trimmed draft as the query to fetch. Returns `None` when the
    /// draft is empty.
    pub fn submit(&mut self) -> Option<String> {
        let query = self.input.trim().to_string();
        if query.is_empty() {
            return None;
        }
        self.query = query.clone();
        self.running = true;
        self.error = None;
        self.selected = 0;
        Some(query)
    }

    pub fn land(&mut self, query: &str, rows: Vec<SearchRow>, total: usize, truncated: bool) {
        if self.query != query {
            return;
        }
        self.results = rows;
        self.total = total;
        self.truncated = truncated;
        self.running = false;
        self.error = None;
        self.selected = 0;
    }

    pub fn fail(&mut self, query: &str, message: String) {
        if self.query != query {
            return;
        }
        self.running = false;
        self.error = Some(message);
    }

    pub fn move_next(&mut self) {
        let len = self.results.len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn move_prev(&mut self) {
        let len = self.results.len();
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn page_down(&mut self) {
        let len = self.results.len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 10).min(len - 1);
    }

    pub fn page_up(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str) -> SearchRow {
        SearchRow {
            id: id.into(),
            kind: "workflow".into(),
            label: format!("label {id}"),
            score: 10,
        }
    }

    #[test]
    fn submit_rejects_empty_draft() {
        let mut palette = SearchPalette::new();
        assert!(palette.submit().is_none());
        palette.push_char('a');
        assert_eq!(palette.submit(), Some("a".to_string()));
        assert!(palette.running());
        assert_eq!(palette.query(), "a");
    }

    #[test]
    fn land_ignores_stale_query() {
        let mut palette = SearchPalette::new();
        palette.push_char('a');
        let query = palette.submit().expect("non-empty draft submits");
        palette.push_char('b');
        let _ = palette.submit();
        palette.land(&query, vec![row("1")], 1, false);
        assert!(palette.results().is_empty());
    }

    #[test]
    fn navigation_wraps_and_pages() {
        let mut palette = SearchPalette::new();
        palette.push_char('q');
        let query = palette.submit().expect("non-empty draft submits");
        let rows = (0..15).map(|i| row(&format!("id-{i}"))).collect::<Vec<_>>();
        palette.land(&query, rows, 15, false);
        assert_eq!(palette.selected(), 0);
        palette.move_prev();
        assert_eq!(palette.selected(), 14);
        palette.move_next();
        assert_eq!(palette.selected(), 0);
        palette.page_down();
        assert_eq!(palette.selected(), 10);
        palette.page_up();
        assert_eq!(palette.selected(), 0);
    }
}
