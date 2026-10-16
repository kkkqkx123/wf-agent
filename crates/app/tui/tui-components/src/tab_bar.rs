//! Textual tab strip helpers shared by management screens.

/// Render a textual tab strip. The active label is wrapped in brackets.
pub fn format_tabs(current: &str, labels: &[&str]) -> String {
    labels
        .iter()
        .map(|label| {
            if *label == current {
                format!("[{label}]")
            } else {
                (*label).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_tab_is_bracketed() {
        assert_eq!(format_tabs("B", &["A", "B", "C"]), "A [B] C");
    }

    #[test]
    fn missing_active_stays_plain() {
        assert_eq!(format_tabs("Z", &["A", "B"]), "A B");
    }
}
