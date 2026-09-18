//! Theme detection: palette derivation, last-known-good cache and user
//! overrides over pure [`Theme`] data.
//!
//! The theme is pure data ([`Theme`]) consumed by the components that map
//! roles to ratatui styles; this module never renders and never touches the
//! terminal. Live OSC 10/11 probing lives in `tui-terminal::probe`; this
//! module only turns injected probe results into a theme via
//! [`resolve_theme`], so tests stay deterministic and the dependency points
//! from style toward the terminal, never toward an async runtime.
//!
//! Resolution order: explicit user file → live OSC probe result (injected by
//! the caller) → last-known-good cache → built-in dark theme, then user
//! overrides. [`probe_theme`] keeps the legacy convenience path by probing
//! through `tui-terminal` synchronously.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Default OSC response wait
pub const OSC_PROBE_TIMEOUT: Duration = Duration::from_millis(100);

// ── pure data ─────────────────────────────────────────────────────────

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
    fn blend(self, other: Rgb, t: f32) -> Rgb {
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

// ── derivation (pure) ─────────────────────────────────────────────────

/// Relative luminance in `0.0..=1.0` (ITU-R BT.601 weights).
pub fn luminance(c: Rgb) -> f32 {
    (0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32) / 255.0
}

/// Accent candidates; the winner maximizes contrast against the background.
const ACCENT_CANDIDATES: [Rgb; 4] = [
    Rgb::new(0x22, 0xD3, 0xEE), // cyan
    Rgb::new(0xA7, 0x8B, 0xFA), // violet
    Rgb::new(0xF5, 0x9E, 0x0B), // amber
    Rgb::new(0x2D, 0xD4, 0xBF), // teal
];

/// Luminance threshold below which a background is "dark".
const DARK_LUMINANCE_THRESHOLD: f32 = 0.5;

/// Derive a full theme from a background color (foreground defaults to the
/// classic white/black contrasting default when `fg` is `None`).
pub fn derive_theme(bg: Rgb, fg: Option<Rgb>) -> Theme {
    let kind = if luminance(bg) < DARK_LUMINANCE_THRESHOLD {
        ThemeKind::Dark
    } else {
        ThemeKind::Light
    };
    let default_fg = match kind {
        ThemeKind::Dark => Rgb::new(0xE5, 0xE7, 0xEB),
        ThemeKind::Light => Rgb::new(0x1F, 0x29, 0x37),
    };
    let fg = fg.unwrap_or(default_fg);

    let bg_lum = luminance(bg);
    let accent = ACCENT_CANDIDATES
        .into_iter()
        .max_by(|a, b| {
            let da = (luminance(*a) - bg_lum).abs();
            let db = (luminance(*b) - bg_lum).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(Rgb::new(0x22, 0xD3, 0xEE));

    // Role variants follow the theme kind (dark → brighter, light → deeper).
    let (add, remove, warning, error, highlight) = match kind {
        ThemeKind::Dark => (
            Rgb::new(0x4A, 0xDE, 0x80),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0xFA, 0xCC, 0x15),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0x60, 0xA5, 0xFA),
        ),
        ThemeKind::Light => (
            Rgb::new(0x15, 0x80, 0x3D),
            Rgb::new(0xB4, 0x23, 0x23),
            Rgb::new(0xA1, 0x62, 0x07),
            Rgb::new(0xB4, 0x23, 0x23),
            Rgb::new(0x1D, 0x4E, 0xD8),
        ),
    };

    Theme {
        kind,
        fg,
        bg,
        muted: fg.blend(bg, 0.55),
        accent,
        add,
        remove,
        warning,
        error,
        highlight,
        source: ThemeSource::Probed,
    }
}

/// Whether the background is light (true) or dark (false).
pub fn is_light(bg: Rgb) -> bool {
    luminance(bg) > DARK_LUMINANCE_THRESHOLD
}

/// Blend two RGB colors with alpha. `alpha = 0.0` → `top`, `alpha = 1.0` → `bottom`.
pub fn blend_rgb(top: Rgb, bottom: Rgb, alpha: f32) -> Rgb {
    top.blend(bottom, alpha)
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

/// Accent style for active/selected TUI controls.
/// Adapts to light/dark backgrounds.
pub fn accent_style(bg: Rgb) -> ratatui::style::Style {
    if is_light(bg) {
        // Use darker cyan on light backgrounds for better contrast
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
    Theme::dark_default().style_for_role(ColorRole::Accent)
}

/// Style for successful operations.
pub fn success_style() -> ratatui::style::Style {
    Theme::dark_default().style_for_role(ColorRole::Add)
}

/// Style for failed operations.
pub fn error_style() -> ratatui::style::Style {
    Theme::dark_default().style_for_role(ColorRole::Error)
}

/// Style for warnings.
pub fn warning_style() -> ratatui::style::Style {
    Theme::dark_default().style_for_role(ColorRole::Warning)
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

// ── literal compatibility ────────────────────────────────────────────

/// Squared Euclidean distance between two colors.
fn color_distance_sq(a: Rgb, b: Rgb) -> u32 {
    let dr = a.r as i32 - b.r as i32;
    let dg = a.g as i32 - b.g as i32;
    let db = a.b as i32 - b.b as i32;
    (dr * dr + dg * dg + db * db) as u32
}

/// Nearest semantic role for a historical literal color, measured against
/// the dark-default palette (the palette every literal was picked from).
/// Returns the role whose default color is closest to `literal`; exact ties
/// resolve to the earlier role in palette order.
pub fn nearest_role(literal: Rgb) -> ColorRole {
    let defaults = Theme::dark_default();
    let candidates = [
        (ColorRole::Default, defaults.fg),
        (ColorRole::Muted, defaults.muted),
        (ColorRole::Accent, defaults.accent),
        (ColorRole::Add, defaults.add),
        (ColorRole::Remove, defaults.remove),
        (ColorRole::Warning, defaults.warning),
        (ColorRole::Error, defaults.error),
        (ColorRole::Highlight, defaults.highlight),
    ];
    candidates
        .into_iter()
        .min_by_key(|(_, color)| color_distance_sq(literal, *color))
        .map(|(role, _)| role)
        .unwrap_or(ColorRole::Default)
}

/// Compatibility mapping for historical literal colors: resolve `literal`
/// to its nearest semantic role, then return that role's color from
/// `theme`. When the theme is untouched the result equals the historical
/// value, so default output stays byte-identical; user overrides flow to
/// every migrated call site without touching draw code.
pub fn remap_literal(literal: Rgb, theme: &Theme) -> Rgb {
    theme.rgb_for_role(nearest_role(literal))
}

// ── OSC response parsing (pure) ───────────────────────────────────────

/// Incremental parser for OSC 10/11 color responses
/// (`ESC ] 11 ; rgb:RRRR/GGGG/BBBB ST`, ST = `ESC \` or BEL). Non-OSC bytes
/// are dropped, so responses split across arbitrary chunk boundaries still
/// parse.
#[derive(Debug, Default)]
pub struct OscColorParser {
    buf: Vec<u8>,
    scan_from: usize,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
}

impl OscColorParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the next chunk of terminal bytes.
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
        self.scan();
    }

    /// Both responses captured.
    pub fn is_complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some()
    }

    /// Extract the captured colors.
    pub fn finish(self) -> (Option<Rgb>, Option<Rgb>) {
        (self.fg, self.bg)
    }

    fn scan(&mut self) {
        loop {
            // Skip garbage up to the next ESC.
            let start = self.buf[self.scan_from..]
                .iter()
                .position(|&b| b == 0x1b)
                .map(|i| self.scan_from + i);
            let Some(start) = start else {
                self.scan_from = self.buf.len();
                self.drain_scanned();
                return;
            };
            self.buf.drain(..start);
            self.scan_from = 0;

            match parse_osc_response(&self.buf) {
                Some((query, rgb, consumed)) => {
                    match query {
                        10 => self.fg = Some(rgb),
                        11 => self.bg = Some(rgb),
                        _ => {}
                    }
                    self.buf.drain(..consumed);
                    if self.is_complete() {
                        return;
                    }
                }
                // Incomplete response at the buffer end → wait for more.
                None if looks_like_incomplete_response(&self.buf) => {
                    self.drain_scanned();
                    return;
                }
                // ESC that is not an OSC color response → skip this byte.
                None => {
                    self.buf.drain(..1);
                }
            }
        }
    }

    /// Drop already-scanned garbage so the buffer stays bounded.
    fn drain_scanned(&mut self) {
        if self.scan_from > 0 && self.scan_from == self.buf.len() {
            self.buf.clear();
            self.scan_from = 0;
        }
    }
}

/// Whether the buffer starts with a potentially-incomplete OSC color
/// response (a prefix of `\x1b]1x;rgb:…`).
fn looks_like_incomplete_response(buf: &[u8]) -> bool {
    let Some(rest) = buf.strip_prefix(b"\x1b]") else {
        return false;
    };
    for header in [&b"10;rgb:"[..], &b"11;rgb:"[..]] {
        if rest.len() < header.len() && header.starts_with(rest) {
            return true; // still receiving the header
        }
        if rest.starts_with(header) {
            return true; // payload / terminator not fully arrived
        }
    }
    false
}

/// Parse one complete OSC color response at the start of `buf`.
/// Returns `(query, rgb, bytes_consumed)`.
pub fn parse_osc_response(buf: &[u8]) -> Option<(u8, Rgb, usize)> {
    let rest = buf.strip_prefix(b"\x1b]")?;
    let (query, rest) = if let Some(r) = rest.strip_prefix(b"10;") {
        (10u8, r)
    } else {
        let r = rest.strip_prefix(b"11;")?;
        (11u8, r)
    };
    let rest = rest.strip_prefix(b"rgb:")?;

    let mut pos = 0usize;
    let mut values = [0u8; 3];
    for (idx, value) in values.iter_mut().enumerate() {
        if idx > 0 {
            if rest.get(pos) == Some(&b'/') {
                pos += 1;
            } else {
                return None;
            }
        }
        let start = pos;
        while pos < rest.len() && (rest[pos] as char).is_ascii_hexdigit() && pos - start < 4 {
            pos += 1;
        }
        let len = pos - start;
        if len == 0 {
            return None;
        }
        let digits = std::str::from_utf8(&rest[start..pos]).ok()?;
        let raw = u16::from_str_radix(digits, 16).ok()?;
        *value = match len {
            1 => (raw as u8) * 17,
            2 => raw as u8,
            3 => ((raw as u32 * 255) / 0xFFF) as u8,
            _ => (raw >> 8) as u8,
        };
    }

    // Terminator: BEL or `ESC \`.
    let terminator_len = match rest.get(pos) {
        Some(0x07) => 1,
        Some(0x1b) if rest.get(pos + 1) == Some(&b'\\') => 2,
        _ => return None,
    };
    // `\x1b]` + `1x;` + `rgb:` + payload + terminator
    let consumed = 2 + 3 + 4 + pos + terminator_len;
    Some((query, Rgb::new(values[0], values[1], values[2]), consumed))
}

// ── last-known-good cache ─────────────────────────────────────────────

/// Cache file path: `$XDG_CACHE_HOME/wf-cli/theme.json` (fallback
/// `$HOME/.cache/wf-cli/theme.json`). `None` when no home is discoverable.
pub fn theme_cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("wf-cli").join("theme.json"))
}

/// Persist a successfully probed theme (best effort; the `source` field is
/// serde-skipped and always loads back as [`ThemeSource::Cached`]).
pub fn save_theme_cache(theme: &Theme) {
    let Some(path) = theme_cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let Ok(payload) = serde_json::to_vec_pretty(theme) else {
        return;
    };
    let _ = std::fs::write(path, payload);
}

/// Load the last-known-good theme, marked [`ThemeSource::Cached`].
pub fn load_theme_cache() -> Option<Theme> {
    let path = theme_cache_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let mut theme: Theme = serde_json::from_str(&raw).ok()?;
    theme.source = ThemeSource::Cached;
    Some(theme)
}

// ── explicit user theme file ──────────────────────────────────────────

/// User theme file path: `$XDG_CONFIG_HOME/wf-cli/theme.json` (fallback
/// `$HOME/.config/wf-cli/theme.json`). `None` when no home is discoverable.
pub fn theme_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("wf-cli").join("theme.json"))
}

/// Load the explicit user theme file, marked [`ThemeSource::File`]. Takes
/// priority over probing: a user edits this file and sends SIGUSR2 to switch
/// themes even when the terminal never answers OSC color queries. A missing
/// or malformed file is ignored and the probe / cache chain still applies.
pub fn load_theme_file() -> Option<Theme> {
    let path = theme_config_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let mut theme: Theme = serde_json::from_str(&raw).ok()?;
    theme.source = ThemeSource::File;
    Some(theme)
}

/// Theme overrides file path: `$XDG_CONFIG_HOME/wf-cli/theme-overrides.json` (fallback
/// `$HOME/.config/wf-cli/theme-overrides.json`). `None` when no home is discoverable.
pub fn theme_overrides_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("wf-cli").join("theme-overrides.json"))
}

/// Load theme overrides from the user's config directory.
pub fn load_theme_overrides() -> Option<ThemeOverrides> {
    let path = theme_overrides_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

// ── live probing ──────────────────────────────────────────────────────

/// Pure resolution over already-loaded inputs. The caller supplies the file,
/// live probe, cache and override values; no filesystem or terminal access
/// happens here, so the full priority chain stays unit-testable.
pub fn resolve_theme(
    file: Option<Theme>,
    probed_bg: Option<Rgb>,
    probed_fg: Option<Rgb>,
    cached: Option<Theme>,
    overrides: Option<ThemeOverrides>,
) -> Theme {
    let mut theme = if let Some(theme) = file {
        theme
    } else if let Some(bg) = probed_bg {
        let theme = derive_theme(bg, probed_fg);
        save_theme_cache(&theme);
        theme
    } else if let Some(cached) = cached {
        let mut cached = cached;
        cached.source = ThemeSource::Cached;
        cached
    } else {
        Theme::dark_default()
    };
    if let Some(overrides) = overrides {
        theme = overrides.apply(theme);
    }
    theme
}

/// Probe the terminal theme with the default timeout; never panics —
/// failures fall back to the cache / built-in dark theme. The live OSC
/// query runs in `tui-terminal`; this crate only maps the result.
pub fn probe_theme() -> Theme {
    probe_theme_with_timeout(OSC_PROBE_TIMEOUT)
}

/// [`probe_theme`] with an explicit timeout.
pub fn probe_theme_with_timeout(timeout: Duration) -> Theme {
    let file = load_theme_file();
    let (probed_bg, probed_fg) = if file.is_none() {
        let (fg, bg) = tui_terminal::probe::probe_osc_colors(timeout);
        (
            bg.map(|c| Rgb::new(c.r, c.g, c.b)),
            fg.map(|c| Rgb::new(c.r, c.g, c.b)),
        )
    } else {
        (None, None)
    };
    let cached = load_theme_cache();
    let overrides = load_theme_overrides();
    resolve_theme(file, probed_bg, probed_fg, cached, overrides)
}

/// Fallback chain: last-known-good cache → built-in dark theme.
pub fn fallback_theme() -> Theme {
    load_theme_cache().unwrap_or_else(Theme::dark_default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that touch process-wide env / signal state.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── parser ────────────────────────────────────────────────────────

    #[test]
    fn parses_standard_4_digit_bg_response() {
        let mut p = OscColorParser::new();
        p.feed(b"\x1b]11;rgb:1e1e/2021/2222\x1b\\");
        let (fg, bg) = p.finish();
        assert_eq!(fg, None);
        assert_eq!(bg, Some(Rgb::new(0x1e, 0x20, 0x22)));
    }

    #[test]
    fn parses_2_digit_and_bel_terminated_response() {
        let mut p = OscColorParser::new();
        p.feed(b"\x1b]10;rgb:ff/fa/fc\x07");
        let (fg, bg) = p.finish();
        assert_eq!(fg, Some(Rgb::new(0xff, 0xfa, 0xfc)));
        assert_eq!(bg, None);
    }

    #[test]
    fn parses_single_digit_channels() {
        let mut p = OscColorParser::new();
        p.feed(b"\x1b]11;rgb:0/8/f\x07");
        let (_, bg) = p.finish();
        assert_eq!(bg, Some(Rgb::new(0, 0x88, 0xff)));
    }

    #[test]
    fn parses_fg_and_bg_in_one_stream() {
        let mut p = OscColorParser::new();
        p.feed(b"\x1b]11;rgb:0000/0000/0000\x07\x1b]10;rgb:ffff/ffff/ffff\x07");
        assert!(p.is_complete());
        let (fg, bg) = p.finish();
        assert_eq!(fg, Some(Rgb::new(0xff, 0xff, 0xff)));
        assert_eq!(bg, Some(Rgb::new(0, 0, 0)));
    }

    #[test]
    fn parses_responses_split_across_chunks_with_noise() {
        let mut p = OscColorParser::new();
        p.feed(b"garbage\x1b]11;rgb:2");
        p.feed(b"a2a/3030/4040");
        p.feed(b"\x1b\\more noise\x1b]10;rgb:e5e5/e7e7/eb");
        assert!(!p.is_complete());
        p.feed(b"eb\x07");
        assert!(p.is_complete());
        let (fg, bg) = p.finish();
        assert_eq!(fg, Some(Rgb::new(0xe5, 0xe7, 0xeb)));
        assert_eq!(bg, Some(Rgb::new(0x2a, 0x30, 0x40)));
    }

    #[test]
    fn empty_feed_yields_nothing() {
        let mut p = OscColorParser::new();
        p.feed(b"");
        p.feed(b"\x1b[Akey events\x1b[1;2R");
        let (fg, bg) = p.finish();
        assert_eq!((fg, bg), (None, None));
    }

    #[test]
    fn truncated_response_is_not_reported() {
        let mut p = OscColorParser::new();
        p.feed(b"\x1b]11;rgb:1234/5678"); // no terminator
        let (fg, bg) = p.finish();
        assert_eq!((fg, bg), (None, None));
    }

    // ── derivation ────────────────────────────────────────────────────

    #[test]
    fn luminance_extremes() {
        assert!(luminance(Rgb::new(0, 0, 0)) < 0.01);
        assert!(luminance(Rgb::new(255, 255, 255)) > 0.99);
    }

    #[test]
    fn dark_background_derives_dark_theme() {
        let t = derive_theme(Rgb::new(0x10, 0x12, 0x14), None);
        assert_eq!(t.kind, ThemeKind::Dark);
        assert_eq!(t.source, ThemeSource::Probed);
        assert_eq!(t.fg, Rgb::new(0xE5, 0xE7, 0xEB)); // default dark fg
                                                      // Accent must contrast more with the bg than the bg luminance span.
        assert!((luminance(t.accent) - luminance(t.bg)).abs() > 0.2);
    }

    #[test]
    fn light_background_derives_light_theme() {
        let t = derive_theme(Rgb::new(0xFA, 0xFB, 0xFC), Some(Rgb::new(0x20, 0x20, 0x20)));
        assert_eq!(t.kind, ThemeKind::Light);
        assert_eq!(t.fg, Rgb::new(0x20, 0x20, 0x20));
    }

    #[test]
    fn literal_helpers_match_their_roles() {
        let defaults = Theme::dark_default();
        assert_eq!(
            tool_call_style(),
            defaults.style_for_role(ColorRole::Accent)
        );
        assert_eq!(success_style(), defaults.style_for_role(ColorRole::Add));
        assert_eq!(error_style(), defaults.style_for_role(ColorRole::Error));
        assert_eq!(warning_style(), defaults.style_for_role(ColorRole::Warning));
    }

    #[test]
    fn nearest_role_recovers_historical_literals() {
        assert_eq!(nearest_role(Rgb::new(0x22, 0xD3, 0xEE)), ColorRole::Accent);
        assert_eq!(nearest_role(Rgb::new(0x4A, 0xDE, 0x80)), ColorRole::Add);
        // The historical red is shared by the remove and error roles; the
        // tie resolves to the earlier role, whose color is identical.
        assert_eq!(nearest_role(Rgb::new(0xF8, 0x71, 0x71)), ColorRole::Remove);
        assert_eq!(nearest_role(Rgb::new(0xFA, 0xCC, 0x15)), ColorRole::Warning);
    }

    #[test]
    fn remap_literal_is_identity_on_defaults() {
        let defaults = Theme::dark_default();
        for literal in [
            Rgb::new(0x22, 0xD3, 0xEE),
            Rgb::new(0x4A, 0xDE, 0x80),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0xE5, 0xE7, 0xEB),
        ] {
            assert_eq!(remap_literal(literal, &defaults), literal);
        }
    }

    #[test]
    fn remap_literal_follows_user_overrides() {
        let mut custom = Theme::dark_default();
        custom.accent = Rgb::new(0x11, 0x22, 0x33);
        assert_eq!(
            remap_literal(Rgb::new(0x22, 0xD3, 0xEE), &custom),
            Rgb::new(0x11, 0x22, 0x33)
        );
    }

    #[test]
    fn role_colors_differ_between_kinds() {
        let dark = derive_theme(Rgb::new(0, 0, 0), None);
        let light = derive_theme(Rgb::new(255, 255, 255), None);
        assert_ne!(dark.add, light.add);
        assert_ne!(dark.highlight, light.highlight);
        assert_eq!(dark.error, Theme::dark_default().error);
    }

    #[test]
    fn muted_is_between_fg_and_bg() {
        let fg = Rgb::new(0xff, 0xff, 0xff);
        let bg = Rgb::new(0x00, 0x00, 0x00);
        let t = derive_theme(bg, Some(fg));
        for channel in [t.muted.r, t.muted.g, t.muted.b] {
            assert!(channel > 0 && channel < 0xff);
        }
    }

    // ── color domain ──────────────────────────────────────────────────

    #[test]
    fn color_domain_detection() {
        assert_eq!(
            ColorDomain::detect(Some("truecolor"), Some("xterm")),
            ColorDomain::TrueColor
        );
        assert_eq!(
            ColorDomain::detect(Some("24bit"), None),
            ColorDomain::TrueColor
        );
        assert_eq!(
            ColorDomain::detect(None, Some("xterm-256color")),
            ColorDomain::Ansi256
        );
        assert_eq!(ColorDomain::detect(None, Some("dumb")), ColorDomain::Ansi16);
        assert_eq!(ColorDomain::detect(None, None), ColorDomain::Ansi16);
    }

    // ── cache ─────────────────────────────────────────────────────────

    #[test]
    fn theme_cache_roundtrip() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", dir.path());

        let probed = derive_theme(Rgb::new(0x11, 0x22, 0x33), Some(Rgb::new(0xee, 0xee, 0xee)));
        save_theme_cache(&probed);

        let cached = load_theme_cache().expect("cache should hit");
        assert_eq!(cached.kind, probed.kind);
        assert_eq!(cached.bg, probed.bg);
        assert_eq!(cached.fg, probed.fg);
        assert_eq!(cached.source, ThemeSource::Cached);

        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn missing_cache_returns_none() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", dir.path());
        assert!(load_theme_cache().is_none());
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn fallback_theme_uses_dark_default_without_cache() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", dir.path());
        assert_eq!(fallback_theme().source, ThemeSource::Default);
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn theme_config_path_uses_xdg_config_home() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let path = theme_config_path().expect("config home is set");
        assert_eq!(path, dir.path().join("wf-cli").join("theme.json"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn explicit_theme_file_wins_over_cache() {
        let _lock = ENV_LOCK.lock().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", cfg_dir.path());
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());

        // A stale last-known-good snapshot (as if a probe ran before).
        save_theme_cache(&Theme::dark_default());
        // The explicit user file requests the light theme.
        let user = Theme::light_default();
        let cfg_path = theme_config_path().unwrap();
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        std::fs::write(cfg_path, serde_json::to_vec_pretty(&user).unwrap()).unwrap();

        // The probe timeout is irrelevant: the file short-circuits first.
        let theme = probe_theme_with_timeout(Duration::from_millis(1));
        assert_eq!(theme.source, ThemeSource::File);
        assert_eq!(theme.kind, ThemeKind::Light);
        assert_eq!(theme.bg, user.bg);

        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
    }

    #[test]
    fn malformed_theme_file_falls_through_to_cache() {
        let _lock = ENV_LOCK.lock().unwrap();
        let cfg_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", cfg_dir.path());
        std::env::set_var("XDG_CACHE_HOME", cache_dir.path());
        save_theme_cache(&Theme::dark_default());
        let cfg_path = theme_config_path().unwrap();
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        std::fs::write(cfg_path, b"not json {").unwrap();

        let theme = probe_theme_with_timeout(Duration::from_millis(1));
        // The malformed file is ignored; with no OSC responder the cached
        // snapshot (CI: the built-in dark theme) is used.
        assert_ne!(theme.source, ThemeSource::File);
        assert_eq!(theme.kind, ThemeKind::Dark);

        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
    }

    // ── resolution ──────────────────────────────────────────────────

    #[test]
    fn resolve_theme_prefers_file_over_probe_and_cache() {
        let file = Theme::light_default();
        let theme = resolve_theme(
            Some(file.clone()),
            Some(Rgb::new(0, 0, 0)),
            None,
            Some(Theme::dark_default()),
            None,
        );
        assert_eq!(theme.bg, file.bg);
    }

    #[test]
    fn resolve_theme_uses_probe_before_cache() {
        let probed_bg = Rgb::new(0x10, 0x12, 0x14);
        let theme = resolve_theme(
            None,
            Some(probed_bg),
            None,
            Some(Theme::light_default()),
            None,
        );
        assert_eq!(theme.source, ThemeSource::Probed);
        assert_eq!(theme.bg, probed_bg);
    }

    #[test]
    fn resolve_theme_applies_overrides_last() {
        let overrides = ThemeOverrides {
            accent: Some(Rgb::new(0x11, 0x22, 0x33)),
            ..ThemeOverrides::default()
        };
        let theme = resolve_theme(
            None,
            None,
            None,
            Some(Theme::dark_default()),
            Some(overrides),
        );
        assert_eq!(theme.accent, Rgb::new(0x11, 0x22, 0x33));
    }

    #[test]
    fn default_palette_snapshot_is_byte_stable() {
        let dark = Theme::dark_default();
        assert_eq!(dark.fg.hex(), "#e5e7eb");
        assert_eq!(dark.bg.hex(), "#0f141a");
        assert_eq!(dark.muted.hex(), "#8b939e");
        assert_eq!(dark.accent.hex(), "#22d3ee");
        assert_eq!(dark.add.hex(), "#4ade80");
        assert_eq!(dark.remove.hex(), "#f87171");
        assert_eq!(dark.warning.hex(), "#facc15");
        assert_eq!(dark.error.hex(), "#f87171");
        assert_eq!(dark.highlight.hex(), "#60a5fa");
        let light = Theme::light_default();
        assert_eq!(light.fg.hex(), "#1f2937");
        assert_eq!(light.bg.hex(), "#fafbfc");
        assert_eq!(light.muted.hex(), "#6b7280");
        assert_eq!(light.accent.hex(), "#0e748c");
        assert_eq!(light.add.hex(), "#15803d");
        assert_eq!(light.remove.hex(), "#b42323");
        assert_eq!(light.warning.hex(), "#a16207");
        assert_eq!(light.error.hex(), "#b42323");
        assert_eq!(light.highlight.hex(), "#1d4ed8");
    }

    // ── probe degradation ─────────────────────────────────────────────

    #[test]
    fn probe_theme_never_panics_and_reports_a_source() {
        let _lock = ENV_LOCK.lock().unwrap();
        // In CI there is no controlling terminal / no OSC responder; on a
        // developer terminal this may legitimately probe. Either way a
        // fully-formed theme must come back.
        let theme = probe_theme_with_timeout(Duration::from_millis(20));
        assert!(
            matches!(theme.kind, ThemeKind::Dark | ThemeKind::Light),
            "probe returned a valid theme kind"
        );
        assert!(!theme.bg.hex().is_empty(), "bg color must be set");
    }
}
