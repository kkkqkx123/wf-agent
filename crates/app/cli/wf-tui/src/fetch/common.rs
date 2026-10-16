//! Shared helpers for the screen fetchers.

use crate::screens::LocalSettings;

/// Client-side cap for fetched detail lines; the viewer shows the full
/// cached text and the pane windows into it.
pub(super) const DETAIL_LINE_CAP: usize = 300;

pub(super) fn items_of(data: &serde_json::Value) -> Vec<serde_json::Value> {
    data.get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| data.as_array().cloned())
        .unwrap_or_default()
}

/// Split a list response into items plus the server truncation flag.
/// Page responses carry `has_more`; cursor responses carry a next cursor.
pub(super) fn page_of(data: &serde_json::Value) -> (Vec<serde_json::Value>, bool) {
    let has_more = data
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || data
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .is_some_and(|c| !c.is_empty())
        || data
            .get("nextCursor")
            .and_then(|v| v.as_str())
            .is_some_and(|c| !c.is_empty());
    (items_of(data), has_more)
}

/// First non-empty string field under any of `keys`, or empty.
pub(super) fn text_of(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }
    String::new()
}

/// Pre-rendered read-only viewer text for a list item.
pub(super) fn pretty_detail(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

pub(super) fn truncate_value(text: &str, max: usize) -> String {
    let mut out: String = text.chars().take(max).collect();
    if out.len() < text.len() {
        out.push_str("...");
    }
    out
}

pub(super) fn cap_detail_lines(mut lines: Vec<String>) -> Vec<String> {
    if lines.len() > DETAIL_LINE_CAP {
        lines.truncate(DETAIL_LINE_CAP);
        lines.push(format!("... truncated to {DETAIL_LINE_CAP} lines"));
    }
    if lines.is_empty() {
        lines.push("(none)".to_string());
    }
    lines
}

/// Snapshot the local config file values shown on the settings screen.
pub(super) fn load_local_settings() -> LocalSettings {
    let path = crate::app_config::config_file_path().map(|p| p.display().to_string());
    let tool_config = crate::tui::input_classify::load_tool_output_config();
    let copy_target = crate::tui::input_classify::load_copy_target();
    let mouse_capture = crate::tui::input_classify::load_mouse_opt_in();
    let session_scrollbar = crate::tui::input_classify::load_session_scrollbar_opt_in();
    let manager = path.as_ref().and_then(|_| {
        crate::app_config::ConfigManager::load(&crate::app_config::config_file_path().unwrap()).ok()
    });
    let appearance_theme = manager
        .as_ref()
        .and_then(|m| m.config().appearance.theme)
        .map(|v| format!("{v:?}").to_lowercase())
        .unwrap_or_else(|| "auto".to_string());
    let motion_performance = manager
        .as_ref()
        .and_then(|m| m.config().motion.performance)
        .map(|v| format!("{v:?}").to_lowercase())
        .unwrap_or_else(|| "auto".to_string());
    let notification_enabled = manager
        .as_ref()
        .and_then(|m| m.config().notifications.enabled);
    LocalSettings {
        config_path: path.unwrap_or_else(|| "unavailable".to_string()),
        mouse_capture,
        session_scrollbar,
        copy_target: format!("{copy_target:?}").to_lowercase(),
        tool_output_enabled: tool_config.enabled,
        tool_output_level: tool_config.level.label().to_string(),
        preview_max_lines: tool_config.preview_max_lines,
        preview_max_chars: tool_config.preview_max_chars,
        appearance_theme,
        motion_performance,
        notification_enabled,
    }
}

/// Render an epoch timestamp; accepts seconds or milliseconds.
pub fn format_ts(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    let (secs, millis) = if ts > 10_000_000_000 {
        (ts / 1000, (ts % 1000) as u32)
    } else {
        (ts, 0u32)
    };
    match chrono::DateTime::from_timestamp(secs, millis * 1_000_000) {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_formats_and_defaults() {
        assert_eq!(format_ts(0), "-");
        assert_eq!(format_ts(-1), "-");
        // Seconds and milliseconds of the same instant must agree.
        let secs = 1_700_000_000_i64;
        assert_eq!(format_ts(secs), format_ts(secs * 1000));
    }
}
