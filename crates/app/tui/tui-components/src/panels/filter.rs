//! Shared typed-filter state for the footer selection panels.
//!
//! The panels own their filter text; this helper keeps the text in one
//! place so every panel filters the same way. The projection onto the
//! panel list is explicit: a panel decides which part of its filter text
//! selects candidates (the command palette filters on the command word and
//! treats the rest as the argument).

use unicode_segmentation::UnicodeSegmentation;

use crate::select::SelectList;

/// Typed filter shared by the selection panels.
#[derive(Debug, Clone, Default)]
pub struct PanelFilter {
    text: String,
}

impl PanelFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The filter text typed so far.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replace the filter text.
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
    }

    /// Append a character.
    pub fn push(&mut self, c: char) {
        self.text.push(c);
    }

    /// Remove the last grapheme.
    pub fn backspace(&mut self) {
        let trimmed: String = self
            .text
            .graphemes(true)
            .rev()
            .skip(1)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        self.text = trimmed;
    }

    /// Drop the filter text.
    pub fn clear(&mut self) {
        self.text.clear();
    }
}

/// Project a filter text onto a list; an empty filter matches every
/// candidate.
pub fn apply<T>(list: &mut SelectList<T>, text: &str) {
    if text.is_empty() {
        list.set_filter(None);
    } else {
        list.set_filter(Some(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::{Group, GroupItem};

    fn list() -> SelectList<u32> {
        SelectList::groups(vec![Group::new(Some("items"))
            .item(GroupItem::new("alpha", 1))
            .item(GroupItem::new("beta", 2))])
    }

    #[test]
    fn the_filter_text_drives_the_list() {
        let mut list = list();
        let mut filter = PanelFilter::new();
        filter.push('b');
        apply(&mut list, filter.text());
        assert_eq!(filter.text(), "b");
        assert_eq!(list.len(), 1);
        filter.backspace();
        apply(&mut list, filter.text());
        assert_eq!(filter.text(), "");
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn backspace_removes_one_grapheme() {
        let mut filter = PanelFilter::new();
        filter.push('ä');
        filter.push('b');
        filter.backspace();
        assert_eq!(filter.text(), "ä");
    }
}
