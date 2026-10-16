//! Theme validation in perceptual space: contrast, distinctness and
//! colorblind safety as configuration warnings.
//!
//! Validation never rewrites a theme automatically. It reports human-readable
//! warnings so a custom palette that would render unreadable text is visible
//! at configuration time instead of mid-session.

use super::data::{ColorRole, Rgb, Theme};
use super::oklab::{
    contrast_ratio, oklab_distance, simulate_deuteranopia, simulate_protanopia,
    simulate_tritanopia,
};

/// Minimum body-text contrast (WCAG AA).
pub const MIN_BODY_CONTRAST: f32 = 4.5;
/// Minimum contrast for large or emphasized roles.
pub const MIN_ROLE_CONTRAST: f32 = 3.0;
/// Minimum OKLab distance between two distinct roles.
pub const MIN_ROLE_DISTANCE: f32 = 0.06;
/// Minimum OKLab distance under colorblind simulation.
pub const MIN_COLORBLIND_DISTANCE: f32 = 0.03;

/// Severity of a theme finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeCheck {
    Contrast,
    Distinctness,
    Colorblind,
}

/// One validation warning with a stable human-readable message.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeWarning {
    pub check: ThemeCheck,
    pub message: String,
}

impl ThemeWarning {
    fn new(check: ThemeCheck, message: String) -> Self {
        Self { check, message }
    }
}

/// Validation outcome: empty warnings mean the theme is readable.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeReport {
    pub warnings: Vec<ThemeWarning>,
}

impl ThemeReport {
    /// True when no warning was raised.
    pub fn is_ok(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Number of warnings raised.
    pub fn len(&self) -> usize {
        self.warnings.len()
    }

    /// Whether the report holds no warning.
    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Short human summary for logs and startup notices.
    pub fn summary(&self) -> String {
        if self.warnings.is_empty() {
            return "theme ok".to_string();
        }
        let mut out = format!("theme warnings ({})", self.warnings.len());
        for warning in &self.warnings {
            out.push_str("; ");
            out.push_str(warning.message.as_str());
        }
        out
    }
}

fn role_name(role: ColorRole) -> &'static str {
    match role {
        ColorRole::Default => "default",
        ColorRole::Muted => "muted",
        ColorRole::Accent => "accent",
        ColorRole::Add => "add",
        ColorRole::Remove => "remove",
        ColorRole::Warning => "warning",
        ColorRole::Error => "error",
        ColorRole::Highlight => "highlight",
    }
}

fn role_color(theme: &Theme, role: ColorRole) -> Rgb {
    theme.rgb_for_role(role)
}

/// Validate a theme, returning readability warnings without rewriting it.
pub fn validate_theme(theme: &Theme) -> ThemeReport {
    let mut warnings = Vec::new();
    check_contrast(theme, &mut warnings);
    check_distinctness(theme, &mut warnings);
    check_colorblind(theme, &mut warnings);
    ThemeReport { warnings }
}

fn check_contrast(theme: &Theme, warnings: &mut Vec<ThemeWarning>) {
    let body = contrast_ratio(theme.fg, theme.bg);
    if body < MIN_BODY_CONTRAST {
        warnings.push(ThemeWarning::new(
            ThemeCheck::Contrast,
            format!("body text contrast {body:.2} below {MIN_BODY_CONTRAST:.1} (fg vs bg)"),
        ));
    }
    for role in [
        ColorRole::Muted,
        ColorRole::Accent,
        ColorRole::Add,
        ColorRole::Remove,
        ColorRole::Warning,
        ColorRole::Error,
        ColorRole::Highlight,
    ] {
        let ratio = contrast_ratio(role_color(theme, role), theme.bg);
        if ratio < MIN_ROLE_CONTRAST {
            warnings.push(ThemeWarning::new(
                ThemeCheck::Contrast,
                format!(
                    "role {} contrast {ratio:.2} below {MIN_ROLE_CONTRAST:.1} vs bg",
                    role_name(role)
                ),
            ));
        }
    }
}

fn check_distinctness(theme: &Theme, warnings: &mut Vec<ThemeWarning>) {
    let roles = [
        ColorRole::Default,
        ColorRole::Muted,
        ColorRole::Accent,
        ColorRole::Add,
        ColorRole::Remove,
        ColorRole::Warning,
        ColorRole::Error,
        ColorRole::Highlight,
    ];
    for (i, first) in roles.iter().enumerate() {
        // Remove and error intentionally share the historical red; comparing
        // them would always warn, so that pair is excluded by design.
        for second in &roles[i + 1..] {
            if matches!(
                (*first, *second),
                (ColorRole::Remove, ColorRole::Error) | (ColorRole::Error, ColorRole::Remove)
            ) {
                continue;
            }
            let distance = oklab_distance(role_color(theme, *first), role_color(theme, *second));
            if distance < MIN_ROLE_DISTANCE {
                warnings.push(ThemeWarning::new(
                    ThemeCheck::Distinctness,
                    format!(
                        "roles {} and {} too close ({distance:.3} < {MIN_ROLE_DISTANCE:.2})",
                        role_name(*first),
                        role_name(*second)
                    ),
                ));
            }
        }
    }
}

fn check_colorblind(theme: &Theme, warnings: &mut Vec<ThemeWarning>) {
    // Only the success-vs-failure pairs are safety-critical: confusing a
    // passing check with a failing one is the dangerous mistake, while
    // warning/amber overlaps stay inside the alert family. The default
    // palettes keep wide margins here; genuinely confusable customs (equal
    // lightness on a confusion line) collapse below the threshold.
    let roles = [
        (ColorRole::Add, "add", ColorRole::Remove, "remove"),
        (ColorRole::Add, "add", ColorRole::Error, "error"),
    ];
    type SimulateFn = fn(Rgb) -> Rgb;
    let simulations: [(&str, SimulateFn); 3] = [
        ("protanopia", simulate_protanopia),
        ("deuteranopia", simulate_deuteranopia),
        ("tritanopia", simulate_tritanopia),
    ];
    for (kind, simulate) in simulations {
        for (first, first_name, second, second_name) in roles {
            let a = simulate(role_color(theme, first));
            let b = simulate(role_color(theme, second));
            let distance = oklab_distance(a, b);
            if distance < MIN_COLORBLIND_DISTANCE {
                warnings.push(ThemeWarning::new(
                    ThemeCheck::Colorblind,
                    format!(
                        "roles {first_name} and {second_name} collide under {kind} ({distance:.3})"
                    ),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_palettes_pass_validation() {
        assert!(validate_theme(&Theme::dark_default()).is_ok());
        assert!(validate_theme(&Theme::light_default()).is_ok());
    }

    #[test]
    fn unreadable_custom_palette_warns() {
        let mut theme = Theme::dark_default();
        theme.fg = theme.bg;
        theme.muted = theme.bg;
        let report = validate_theme(&theme);
        assert!(!report.is_ok());
        assert!(report.warnings.iter().any(|w| w.check == ThemeCheck::Contrast));
        assert!(!report.summary().is_empty());
    }

    #[test]
    fn colliding_roles_warn_distinctness() {
        let mut theme = Theme::dark_default();
        theme.accent = theme.add;
        let report = validate_theme(&theme);
        assert!(report.warnings.iter().any(|w| w.check == ThemeCheck::Distinctness));
    }

    #[test]
    fn confusion_line_pair_warns_colorblind() {
        // Pink vs light green: far apart for normal vision but nearly
        // identical under protanopia (equal lightness on a confusion line).
        let mut theme = Theme::dark_default();
        theme.add = Rgb::new(0x99, 0xEE, 0x66);
        theme.remove = Rgb::new(0xFF, 0x22, 0xAA);
        theme.error = Rgb::new(0xFF, 0x22, 0xAA);
        let report = validate_theme(&theme);
        assert!(report.warnings.iter().any(|w| w.check == ThemeCheck::Colorblind));
    }
}
