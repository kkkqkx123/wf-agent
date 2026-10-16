//! Session list browser state: selection over the open chat tabs.
//!
//! The browser is transient and never enters the navigation stack. It owns
//! only the selected row so any screen, including the session screen, can
//! open it without leaving the current page. Rendering reads the live
//! [`crate::session_tabs::SessionTabs`] snapshot directly.

/// Session list selection state.
#[derive(Debug, Default)]
pub struct SessionList {
    selected: usize,
}

impl SessionList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Clamp the selection into a list of `len` rows.
    pub fn open(&mut self, active: usize, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = active.min(len - 1);
        }
    }

    pub fn select(&mut self, index: usize, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = index.min(len - 1);
        }
    }

    pub fn move_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn move_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_clamps_active_into_range() {
        let mut list = SessionList::new();
        list.open(5, 3);
        assert_eq!(list.selected(), 2);
        list.open(1, 3);
        assert_eq!(list.selected(), 1);
    }

    #[test]
    fn navigation_wraps() {
        let mut list = SessionList::new();
        list.open(0, 3);
        list.move_prev(3);
        assert_eq!(list.selected(), 2);
        list.move_next(3);
        assert_eq!(list.selected(), 0);
        list.select(9, 3);
        assert_eq!(list.selected(), 2);
    }
}
