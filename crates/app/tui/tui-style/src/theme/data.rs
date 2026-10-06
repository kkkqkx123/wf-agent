//! Pure theme data structures: Rgb, ThemeKind, ThemeSource, ColorDomain,
//! ColorRole, Theme, and ThemeOverrides.

use serde::{Deserialize, Serialize};

/// 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Hex form `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Linear-ish blend: `t = 0` → `self`, `t = 1` → `other`.
    pub(crate) fn blend(self, other: Rgb, t: f32) -> Rgb {
        let mix = |a: u8, b: u8| {
            (a as f32 + (b as f32 - a as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgb::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
        )
    }
}

/// Dark or light theme, derived from the background luminance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Dark,
    Light,
}

/// How the returned theme came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeSource {
    /// Explicit user theme file (highest priority).
    File,
    /// Live OSC 10/11 response.
    Probed,
    /// Last-known-good cache file.
    Cached,
    /// Built-in fallback.
    #[default]
    Default,
}

/// Color-domain capability for later ANSI mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDomain {
    TrueColor,
    Ansi256,
    Ansi16,
}

/// Semantic color role for a history line or UI element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorRole {
    /// Default text/foreground.
    #[default]
    Default,
    /// Muted/dimmed secondary text.
    Muted,
    /// Accent/brand emphasis.
    Accent,
    /// Additions (diff +, success).
    Add,
    /// Removals (diff -).
    Remove,
    /// Warnings.
    Warning,
    /// Errors.
    Error,
    /// Highlights/selection.
    Highlight,
}

impl ColorDomain {
    /// Detect from `COLORTERM` / `TERM` (pure; env injected for tests).
    pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Self {
        match colorterm.map(str::to_ascii_lowercase).as_deref() {
            Some("truecolor") | Some("24bit") => return Self::TrueColor,
            _ => {}
        }
        if term.map(|t| t.contains("256color")).unwrap_or(false) {
            Self::Ansi256
        } else {
            Self::Ansi16
        }
    }

    /// Detect from the real environment.
    pub fn detect_from_env() -> Self {
        Self::detect(
            std::env::var("COLORTERM").ok().as_deref(),
            std::env::var("TERM").ok().as_deref(),
        )
    }

    /// Whether this color domain supports RGB colors.
    pub fn supports_rgb(&self) -> bool {
        matches!(self, Self::TrueColor)
    }

    /// Whether this color domain supports 256 colors.
    pub fn supports_256(&self) -> bool {
        matches!(self, Self::TrueColor | Self::Ansi256)
    }
}

/// Terminal theme: the 8 ColorRoles as pure RGB data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub kind: ThemeKind,
    /// Default role (text).
    pub fg: Rgb,
    /// Canvas.
    pub bg: Rgb,
    /// Muted role (dimmed text).
    pub muted: Rgb,
    /// Brand / accent role.
    pub accent: Rgb,
    /// Additions (diff +, success).
    pub add: Rgb,
    /// Removals (diff -).
    pub remove: Rgb,
    /// Warnings.
    pub warning: Rgb,
    /// Errors.
    pub error: Rgb,
    /// Highlights / selection.
    pub highlight: Rgb,
    #[serde(skip)]
    pub source: ThemeSource,
}

/// User-configurable theme overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeOverrides {
    /// Override for the foreground color.
    pub fg: Option<Rgb>,
    /// Override for the background color.
    pub bg: Option<Rgb>,
    /// Override for the muted color.
    pub muted: Option<Rgb>,
    /// Override for the accent color.
    pub accent: Option<Rgb>,
    /// Override for the add/success color.
    pub add: Option<Rgb>,
    /// Override for the remove/error color.
    pub remove: Option<Rgb>,
    /// Override for the warning color.
    pub warning: Option<Rgb>,
    /// Override for the error color.
    pub error: Option<Rgb>,
    /// Override for the highlight color.
    pub highlight: Option<Rgb>,
}

impl ThemeOverrides {
    /// Apply these overrides to a theme, returning a new theme with the overrides applied.
    pub fn apply(&self, mut theme: Theme) -> Theme {
        if let Some(fg) = self.fg {
            theme.fg = fg;
        }
        if let Some(bg) = self.bg {
            theme.bg = bg;
        }
        if let Some(muted) = self.muted {
            theme.muted = muted;
        }
        if let Some(accent) = self.accent {
            theme.accent = accent;
        }
        if let Some(add) = self.add {
            theme.add = add;
        }
        if let Some(remove) = self.remove {
            theme.remove = remove;
        }
        if let Some(warning) = self.warning {
            theme.warning = warning;
        }
        if let Some(error) = self.error {
            theme.error = error;
        }
        if let Some(highlight) = self.highlight {
            theme.highlight = highlight;
        }
        theme
    }
}

impl Theme {
    /// Get the ANSI color for a color role based on terminal capabilities.
    /// Returns an ANSI color code that provides best compatibility.
    pub fn ansi_color_for_role(&self, role: ColorRole) -> ratatui::style::Color {
        match role {
            ColorRole::Default => ratatui::style::Color::Reset,
            ColorRole::Muted => ratatui::style::Color::DarkGray,
            ColorRole::Accent => ratatui::style::Color::Cyan,
            ColorRole::Add => ratatui::style::Color::Green,
            ColorRole::Remove => ratatui::style::Color::Red,
            ColorRole::Warning => ratatui::style::Color::Yellow,
            ColorRole::Error => ratatui::style::Color::Red,
            ColorRole::Highlight => ratatui::style::Color::Cyan,
        }
    }

    /// Get the ratatui style for a color role, using RGB colors from the theme.
    /// This preserves the current visual appearance while providing semantic meaning.
    pub fn style_for_role(&self, role: ColorRole) -> ratatui::style::Style {
        let color = match role {
            ColorRole::Default => self.fg,
            ColorRole::Muted => self.muted,
            ColorRole::Accent => self.accent,
            ColorRole::Add => self.add,
            ColorRole::Remove => self.remove,
            ColorRole::Warning => self.warning,
            ColorRole::Error => self.error,
            ColorRole::Highlight => self.highlight,
        };
        ratatui::style::Style::default().fg(to_ratatui_color(color))
    }

    /// Get the style for a color role, using ANSI colors if RGB is not supported.
    /// This provides fallback for terminals with limited color support.
    pub fn style_for_role_with_fallback(
        &self,
        role: ColorRole,
        color_domain: ColorDomain,
    ) -> ratatui::style::Style {
        if color_domain.supports_rgb() {
            self.style_for_role(role)
        } else {
            ratatui::style::Style::default().fg(self.ansi_color_for_role(role))
        }
    }

    /// Get the bold style for a color role.
    pub fn bold_style_for_role(&self, role: ColorRole) -> ratatui::style::Style {
        self.style_for_role(role)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    /// Get the dim style for a color role.
    pub fn dim_style_for_role(&self, role: ColorRole) -> ratatui::style::Style {
        self.style_for_role(role)
            .add_modifier(ratatui::style::Modifier::DIM)
    }

    /// Get the foreground color as a ratatui Color.
    pub fn fg(&self) -> ratatui::style::Color {
        to_ratatui_color(self.fg)
    }

    /// Get the background color as a ratatui Color.
    pub fn bg(&self) -> ratatui::style::Color {
        to_ratatui_color(self.bg)
    }

    /// Get the RGB color for a color role.
    pub fn rgb_for_role(&self, role: ColorRole) -> Rgb {
        match role {
            ColorRole::Default => self.fg,
            ColorRole::Muted => self.muted,
            ColorRole::Accent => self.accent,
            ColorRole::Add => self.add,
            ColorRole::Remove => self.remove,
            ColorRole::Warning => self.warning,
            ColorRole::Error => self.error,
            ColorRole::Highlight => self.highlight,
        }
    }
}

impl Theme {
    /// Built-in dark fallback.
    pub fn dark_default() -> Self {
        Self {
            kind: ThemeKind::Dark,
            fg: Rgb::new(0xE5, 0xE7, 0xEB),
            bg: Rgb::new(0x0F, 0x14, 0x1A),
            muted: Rgb::new(0x8B, 0x93, 0x9E),
            accent: Rgb::new(0x22, 0xD3, 0xEE),
            add: Rgb::new(0x4A, 0xDE, 0x80),
            remove: Rgb::new(0xF8, 0x71, 0x71),
            warning: Rgb::new(0xFA, 0xCC, 0x15),
            error: Rgb::new(0xF8, 0x71, 0x71),
            highlight: Rgb::new(0x60, 0xA5, 0xFA),
            source: ThemeSource::Default,
        }
    }

    /// Built-in light fallback.
    pub fn light_default() -> Self {
        Self {
            kind: ThemeKind::Light,
            fg: Rgb::new(0x1F, 0x29, 0x37),
            bg: Rgb::new(0xFA, 0xFB, 0xFC),
            muted: Rgb::new(0x6B, 0x72, 0x80),
            accent: Rgb::new(0x0E, 0x74, 0x8C),
            add: Rgb::new(0x15, 0x80, 0x3D),
            remove: Rgb::new(0xB4, 0x23, 0x23),
            warning: Rgb::new(0xA1, 0x62, 0x07),
            error: Rgb::new(0xB4, 0x23, 0x23),
            highlight: Rgb::new(0x1D, 0x4E, 0xD8),
            source: ThemeSource::Default,
        }
    }
}

/// Convert an Rgb color to a ratatui `Color`.
pub fn to_ratatui_color(rgb: Rgb) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Convert an Rgb color to a ratatui `Style` with foreground color.
pub fn to_style(rgb: Rgb) -> ratatui::style::Style {
    ratatui::style::Style::default().fg(to_ratatui_color(rgb))
}

/// Convert an Rgb color to a ratatui `Style` with bold foreground color.
pub fn to_bold_style(rgb: Rgb) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(to_ratatui_color(rgb))
        .add_modifier(ratatui::style::Modifier::BOLD)
}

/// Convert an Rgb color to a ratatui `Style` with dim foreground color.
pub fn to_dim_style(rgb: Rgb) -> ratatui::style::Style {
    ratatui::style::Style::default()
        .fg(to_ratatui_color(rgb))
        .add_modifier(ratatui::style::Modifier::DIM)
}
