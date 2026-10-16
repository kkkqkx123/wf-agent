//! Aligned key/value property helpers for detail panes.

/// Render a list of property lines with a shared colon.
pub fn format_properties(pairs: &[(String, String)]) -> Vec<String> {
    pairs
        .iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect()
}

/// Render a boolean toggle line with a stable textual marker.
pub fn format_toggle(label: &str, enabled: bool) -> String {
    let marker = if enabled { "[x]" } else { "[ ]" };
    let state = if enabled { "on" } else { "off" };
    format!("{marker} {label}: {state}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_preserve_order() {
        let pairs = vec![("id".into(), "1".into()), ("name".into(), "demo".into())];
        assert_eq!(format_properties(&pairs), vec!["id: 1", "name: demo"]);
    }

    #[test]
    fn toggle_uses_checkbox_marker() {
        assert_eq!(
            format_toggle("mouse capture", true),
            "[x] mouse capture: on"
        );
        assert_eq!(
            format_toggle("mouse capture", false),
            "[ ] mouse capture: off"
        );
    }
}
