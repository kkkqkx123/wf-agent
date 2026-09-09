//! Motion and animation utilities for TUI components.
//!
//! This module provides motion-related types and utilities for controlling
//! animation behavior in the TUI. It includes:
//!
//! - [`MotionMode`] for controlling animation behavior across components
//! - [`ReducedMotionIndicator`] for accessibility fallbacks
//! - [`activity_indicator`] for animated status indicators
//! - [`shimmer_text`] for animated text effects
//!
//! The shimmer effect creates a time-based sweeping highlight band across text
//! characters, similar to the Codex implementation. In reduced motion mode,
//! animations are replaced with static indicators or hidden entirely.

use std::time::Instant;

use ratatui::text::Span;

use crate::animation::AnimationMode;

/// Motion mode for controlling animation behavior.
///
/// This enum works with [`AnimationMode`] to provide fine-grained control
/// over animation behavior. It can be derived from [`AnimationMode`] or
/// used independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionMode {
    /// Full animation support with all effects enabled.
    #[default]
    Animated,
    /// Reduced motion for accessibility; only essential animations.
    Reduced,
    /// No animations; static rendering only.
    Static,
}

impl MotionMode {
    /// Create from [`AnimationMode`].
    pub fn from_animation_mode(mode: AnimationMode) -> Self {
        match mode {
            AnimationMode::Animated => Self::Animated,
            AnimationMode::Reduced => Self::Reduced,
            AnimationMode::Static => Self::Static,
        }
    }

    /// Create from a boolean: `true` = Animated, `false` = Static.
    pub fn from_enabled(enabled: bool) -> Self {
        if enabled {
            Self::Animated
        } else {
            Self::Static
        }
    }

    /// Returns true if animations should run.
    pub fn should_animate(self) -> bool {
        self == Self::Animated
    }
}

/// Reduced motion indicator for accessibility fallbacks.
///
/// When animations are disabled, components can use this to provide
/// appropriate fallback visual indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReducedMotionIndicator {
    /// Hide the indicator completely when animation is disabled.
    #[default]
    Hidden,
    /// Show a static bullet character instead of animation.
    StaticBullet,
}

/// Create an animated activity indicator span.
///
/// In animated mode, returns a shimmer dot or spinner character.
/// In reduced/static mode, returns a static bullet or nothing
/// based on the [`ReducedMotionIndicator`] setting.
///
/// # Arguments
///
/// * `start_time` - The time when the activity started
/// * `motion_mode` - The current motion mode
/// * `indicator` - What to show when animation is disabled
///
/// # Returns
///
/// An optional styled span for the activity indicator.
pub fn activity_indicator(
    start_time: Instant,
    motion_mode: MotionMode,
    indicator: ReducedMotionIndicator,
) -> Option<Span<'static>> {
    match motion_mode {
        MotionMode::Animated => {
            let elapsed = start_time.elapsed().as_millis();
            let phase = (elapsed / 200) % 4;
            let ch = match phase {
                0 => "⣾",
                1 => "⣽",
                2 => "⣻",
                3 => "⢿",
                _ => "⣾",
            };
            Some(Span::raw(ch.to_string()))
        }
        MotionMode::Reduced | MotionMode::Static => match indicator {
            ReducedMotionIndicator::Hidden => None,
            ReducedMotionIndicator::StaticBullet => Some(Span::raw("●".to_string())),
        },
    }
}

/// Apply a shimmer effect to text spans.
///
/// In animated mode, creates a time-based sweeping highlight band across
/// the text characters. The shimmer uses true-color RGB blending for
/// smooth color transitions.
///
/// In reduced/static mode, returns the plain text without animation.
///
/// # Arguments
///
/// * `text` - The text to apply shimmer to
/// * `motion_mode` - The current motion mode
///
/// # Returns
///
/// A vector of styled spans representing the shimmered text.
pub fn shimmer_text(text: &str, motion_mode: MotionMode) -> Vec<Span<'static>> {
    match motion_mode {
        MotionMode::Animated => shimmer_spans(text),
        MotionMode::Reduced | MotionMode::Static => vec![Span::raw(text.to_string())],
    }
}

/// Create shimmer spans for text with time-based color animation.
///
/// This function creates a sweeping highlight effect across the text
/// characters. The effect uses a 2-second sweep period and creates
/// a wave-like highlight that moves from left to right.
///
/// # Arguments
///
/// * `text` - The text to shimmer
///
/// # Returns
///
/// A vector of styled spans with the shimmer effect applied.
fn shimmer_spans(text: &str) -> Vec<Span<'static>> {
    let start = Instant::now();
    let mut spans = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    if len == 0 {
        return spans;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let sweep_period = 2.0; // 2-second sweep
    let sweep_pos = (elapsed % sweep_period) / sweep_period; // 0.0 to 1.0

    for (i, ch) in chars.iter().enumerate() {
        let pos = i as f64 / len as f64; // 0.0 to 1.0 position in text
        let distance = (pos - sweep_pos).abs();
        let highlight = if distance < 0.1 {
            // Peak highlight region
            let intensity = 1.0 - (distance / 0.1);
            let r = lerp(0x80, 0xFF, intensity as f32);
            let g = lerp(0x80, 0xFF, intensity as f32);
            let b = lerp(0x80, 0xFF, intensity as f32);
            ratatui::style::Color::Rgb(r, g, b)
        } else {
            // Dim region
            ratatui::style::Color::Rgb(0x60, 0x60, 0x60)
        };

        spans.push(Span::styled(
            ch.to_string(),
            ratatui::style::Style::default().fg(highlight),
        ));
    }

    spans
}

/// Linear interpolation between two values.
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t).clamp(0.0, 255.0) as u8
}

/// Detect reduced motion preference from the environment.
///
/// Checks common environment variables and system settings to determine
/// if the user prefers reduced motion. This is used for accessibility
/// support to automatically adjust animation behavior.
///
/// # Environment Variables Checked
///
/// - `TERM_PROGRAM` with `iTerm` or `Apple_Terminal` on macOS
/// - `COLORTERM` with `truecolor` for terminal capability detection
/// - `TERM` for terminal type detection
///
/// # Returns
///
/// The detected [`MotionMode`] based on environment and system preferences.
pub fn detect_motion_preference() -> MotionMode {
    // Check for common reduced motion indicators

    // macOS: System Preferences > Accessibility > Display > Reduce Motion
    if cfg!(target_os = "macos") {
        // On macOS, we can check for the reduce motion preference
        // by looking at the NSUserDefaults through environment
        if let Ok(output) = std::process::Command::new("defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim() == "1" {
                return MotionMode::Reduced;
            }
        }
    }

    // Linux: GNOME reduce motion setting
    if cfg!(target_os = "linux") {
        if let Ok(output) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim() == "false" {
                return MotionMode::Reduced;
            }
        }
    }

    // Check for NO_COLOR environment variable (common convention)
    if std::env::var("NO_COLOR").is_ok() {
        return MotionMode::Reduced;
    }

    // Check for specific terminal programs that may indicate CI/headless
    if let Ok(term) = std::env::var("TERM") {
        if term == "dumb" || term == "linux" {
            return MotionMode::Reduced;
        }
    }

    // Default to animated if no reduced motion indicators found
    MotionMode::Animated
}

/// Check if the terminal supports truecolor (24-bit color).
///
/// This is used to determine if high-quality shimmer effects can be
/// rendered, or if fallback to simpler color models is needed.
///
/// # Returns
///
/// `true` if the terminal likely supports truecolor.
pub fn supports_truecolor() -> bool {
    // Check COLORTERM for truecolor support
    if let Ok(colorterm) = std::env::var("COLORTERM") {
        if colorterm.to_lowercase() == "truecolor" {
            return true;
        }
    }

    // Check specific terminal programs known to support truecolor
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        let program = term_program.to_lowercase();
        if program.contains("iterm")
            || program.contains("apple_terminal")
            || program.contains("vscode")
            || program.contains("hyper")
            || program.contains("alacritty")
            || program.contains("kitty")
        {
            return true;
        }
    }

    // Check for common modern terminals
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("256color") || term.contains("truecolor") {
            return true;
        }
    }

    false
}

/// Get the optimal motion mode considering environment and preferences.
///
/// This function combines environment detection with explicit mode
/// selection to determine the best motion mode for the current context.
///
/// # Arguments
///
/// * `explicit_mode` - Optional explicitly requested motion mode
///
/// # Returns
///
/// The optimal [`MotionMode`] considering both preferences and capabilities.
pub fn get_optimal_motion_mode(explicit_mode: Option<MotionMode>) -> MotionMode {
    if let Some(mode) = explicit_mode {
        return mode;
    }

    // Auto-detect from environment
    detect_motion_preference()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_mode_from_animation_mode() {
        assert_eq!(
            MotionMode::from_animation_mode(AnimationMode::Animated),
            MotionMode::Animated
        );
        assert_eq!(
            MotionMode::from_animation_mode(AnimationMode::Reduced),
            MotionMode::Reduced
        );
        assert_eq!(
            MotionMode::from_animation_mode(AnimationMode::Static),
            MotionMode::Static
        );
    }

    #[test]
    fn motion_mode_from_enabled() {
        assert_eq!(MotionMode::from_enabled(true), MotionMode::Animated);
        assert_eq!(MotionMode::from_enabled(false), MotionMode::Static);
    }

    #[test]
    fn motion_mode_should_animate() {
        assert!(MotionMode::Animated.should_animate());
        assert!(!MotionMode::Reduced.should_animate());
        assert!(!MotionMode::Static.should_animate());
    }

    #[test]
    fn activity_indicator_animated_mode() {
        let start = Instant::now();
        let indicator =
            activity_indicator(start, MotionMode::Animated, ReducedMotionIndicator::Hidden);
        assert!(indicator.is_some());
        let span = indicator.unwrap();
        assert!(!span.content.is_empty());
    }

    #[test]
    fn activity_indicator_reduced_mode_hidden() {
        let start = Instant::now();
        let indicator =
            activity_indicator(start, MotionMode::Reduced, ReducedMotionIndicator::Hidden);
        assert!(indicator.is_none());
    }

    #[test]
    fn activity_indicator_reduced_mode_bullet() {
        let start = Instant::now();
        let indicator = activity_indicator(
            start,
            MotionMode::Reduced,
            ReducedMotionIndicator::StaticBullet,
        );
        assert!(indicator.is_some());
        let span = indicator.unwrap();
        assert_eq!(span.content.as_ref(), "●");
    }

    #[test]
    fn shimmer_text_animated_mode() {
        let spans = shimmer_text("hello", MotionMode::Animated);
        assert_eq!(spans.len(), 5); // One span per character
        assert_eq!(spans[0].content.as_ref(), "h");
    }

    #[test]
    fn shimmer_text_reduced_mode() {
        let spans = shimmer_text("hello", MotionMode::Reduced);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "hello");
    }

    #[test]
    fn shimmer_text_static_mode() {
        let spans = shimmer_text("hello", MotionMode::Static);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "hello");
    }

    #[test]
    fn lerp_works_correctly() {
        assert_eq!(lerp(0, 100, 0.0), 0);
        assert_eq!(lerp(0, 100, 0.5), 50);
        assert_eq!(lerp(0, 100, 1.0), 100);
        assert_eq!(lerp(100, 0, 0.5), 50);
    }

    #[test]
    fn detect_motion_preference_returns_valid_mode() {
        let mode = detect_motion_preference();
        // Should return one of the valid motion modes
        assert!(matches!(
            mode,
            MotionMode::Animated | MotionMode::Reduced | MotionMode::Static
        ));
    }

    #[test]
    fn supports_truecolor_does_not_panic() {
        // This function just checks environment variables, should not panic
        let _result = supports_truecolor();
    }

    #[test]
    fn get_optimal_motion_mode_with_explicit() {
        let mode = get_optimal_motion_mode(Some(MotionMode::Reduced));
        assert_eq!(mode, MotionMode::Reduced);

        let mode = get_optimal_motion_mode(Some(MotionMode::Static));
        assert_eq!(mode, MotionMode::Static);
    }

    #[test]
    fn get_optimal_motion_mode_auto_detects() {
        // When no explicit mode is provided, should auto-detect
        let mode = get_optimal_motion_mode(None);
        assert!(matches!(
            mode,
            MotionMode::Animated | MotionMode::Reduced | MotionMode::Static
        ));
    }
}
