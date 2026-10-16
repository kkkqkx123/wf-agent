//! Boolean toggle row helpers shared by settings and context sidebars.

use crate::property_list::format_toggle as format_toggle_text;

/// A boolean toggle displayed as a checkbox row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleRow {
    pub label: String,
    pub enabled: bool,
    pub hint: Option<String>,
}

impl ToggleRow {
    pub fn new(label: impl Into<String>, enabled: bool) -> Self {
        Self {
            label: label.into(),
            enabled,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn marker(&self) -> &'static str {
        if self.enabled {
            "[x]"
        } else {
            "[ ]"
        }
    }

    pub fn state_text(&self) -> &'static str {
        if self.enabled {
            "on"
        } else {
            "off"
        }
    }

    pub fn render_text(&self) -> String {
        let mut text = format_toggle_text(&self.label, self.enabled);
        if let Some(hint) = &self.hint {
            text.push_str(&format!("  ({hint})"));
        }
        text
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}

/// Collection helper for rendering multiple toggles.
pub fn render_toggles(rows: &[ToggleRow]) -> Vec<String> {
    rows.iter().map(ToggleRow::render_text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_markers_flip() {
        let mut row = ToggleRow::new("mouse capture", false);
        assert_eq!(row.marker(), "[ ]");
        row.toggle();
        assert_eq!(row.marker(), "[x]");
        assert_eq!(row.state_text(), "on");
    }

    #[test]
    fn render_includes_hint() {
        let row = ToggleRow::new("wrap", true).with_hint("Ctrl+W");
        assert_eq!(row.render_text(), "[x] wrap: on  (Ctrl+W)");
    }

    #[test]
    fn collection_render_is_stable() {
        let rows = vec![ToggleRow::new("a", true), ToggleRow::new("b", false)];
        assert_eq!(render_toggles(&rows), vec!["[x] a: on", "[ ] b: off"]);
    }
}
