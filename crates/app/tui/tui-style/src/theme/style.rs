//! Style helper functions for common UI elements.

use super::data::{to_bold_style, to_dim_style, to_ratatui_color, Rgb};
use super::derive::{blend_rgb, is_light};

/// Accent style for active/selected TUI controls.
/// Adapts to light/dark backgrounds.
pub fn accent_style(bg: Rgb) -> ratatui::style::Style {
    if is_light(bg) {
        to_bold_style(Rgb::new(0x00, 0x5F, 0x87))
    } else {
        to_bold_style(Rgb::new(0x22, 0xD3, 0xEE))
    }
}

/// Style for user-authored messages. Generates a subtle background blend.
pub fn user_message_style(bg: Rgb) -> ratatui::style::Style {
    let blended = if is_light(bg) {
        blend_rgb(Rgb::new(0, 0, 0), bg, 0.04)
    } else {
        blend_rgb(Rgb::new(255, 255, 255), bg, 0.12)
    };
    ratatui::style::Style::default().bg(to_ratatui_color(blended))
}

/// Style for assistant text (default style).
pub fn assistant_message_style() -> ratatui::style::Style {
    ratatui::style::Style::default()
}

/// Style for tool call indicators.
pub fn tool_call_style() -> ratatui::style::Style {
    super::data::Theme::dark_default().style_for_role(super::data::ColorRole::Accent)
}

/// Style for successful operations.
pub fn success_style() -> ratatui::style::Style {
    super::data::Theme::dark_default().style_for_role(super::data::ColorRole::Add)
}

/// Style for failed operations.
pub fn error_style() -> ratatui::style::Style {
    super::data::Theme::dark_default().style_for_role(super::data::ColorRole::Error)
}

/// Style for warnings.
pub fn warning_style() -> ratatui::style::Style {
    super::data::Theme::dark_default().style_for_role(super::data::ColorRole::Warning)
}

/// Style for muted/dimmed text.
pub fn muted_style() -> ratatui::style::Style {
    ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::DIM)
}

/// Style for highlights/selection.
pub fn highlight_style(_bg: Rgb) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(to_ratatui_color(Rgb::new(0x00, 0x00, 0x00)))
        .bg(to_ratatui_color(Rgb::new(0x22, 0xD3, 0xEE)))
        .add_modifier(ratatui::style::Modifier::BOLD)
}

/// Low-contrast rule style for separators within markdown tables.
pub fn table_separator_style(fg: Rgb, bg: Rgb) -> ratatui::style::Style {
    let blended = blend_rgb(fg, bg, 0.20);
    to_dim_style(blended)
}
