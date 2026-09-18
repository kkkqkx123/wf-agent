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

use ratatui::text::Span;

use crate::animation::AnimationMode;

/// Braille spinner frames indexed by quarter-phase; shared with the footer
/// spinner so both indicators step through the same sequence.
pub const BRAILLE_FRAMES: [&str; 4] = ["⣾", "⣽", "⣻", "⢿"];

/// Shimmer sweep period in milliseconds.
pub const SHIMMER_PERIOD_MS: u64 = 2_000;
/// Shimmer highlight half-width as a fraction of the text length.
pub const SHIMMER_HALF_WIDTH_FRAC: f64 = 0.1;

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
/// * `start_ms` - Clock value (ms) when the activity started
/// * `now_ms` - Current clock value (ms); callers pass the injectable clock
/// * `motion_mode` - The current motion mode
/// * `indicator` - What to show when animation is disabled
///
/// # Returns
///
/// An optional styled span for the activity indicator.
pub fn activity_indicator_at(
    start_ms: u64,
    now_ms: u64,
    motion_mode: MotionMode,
    indicator: ReducedMotionIndicator,
) -> Option<Span<'static>> {
    match motion_mode {
        MotionMode::Animated => {
            let elapsed = now_ms.saturating_sub(start_ms);
            let phase = (elapsed / 200) % BRAILLE_FRAMES.len() as u64;
            let ch = BRAILLE_FRAMES[phase as usize % BRAILLE_FRAMES.len()];
            Some(Span::raw(ch.to_string()))
        }
        MotionMode::Reduced | MotionMode::Static => match indicator {
            ReducedMotionIndicator::Hidden => None,
            ReducedMotionIndicator::StaticBullet => Some(Span::raw("●".to_string())),
        },
    }
}

/// Legacy wrapper stamping the activity start at the current injectable
/// clock value.
pub fn activity_indicator(
    start_ms: u64,
    motion_mode: MotionMode,
    indicator: ReducedMotionIndicator,
) -> Option<Span<'static>> {
    activity_indicator_at(start_ms, tui_clock::clock::now_ms(), motion_mode, indicator)
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
/// * `now_ms` - Current clock value (ms) driving the sweep position
///
/// # Returns
///
/// A vector of styled spans representing the shimmered text.
pub fn shimmer_text_at(text: &str, motion_mode: MotionMode, now_ms: u64) -> Vec<Span<'static>> {
    match motion_mode {
        MotionMode::Animated => shimmer_spans_at(text, now_ms),
        MotionMode::Reduced | MotionMode::Static => vec![Span::raw(text.to_string())],
    }
}

/// Wrapper reading the sweep position from the injectable clock.
pub fn shimmer_text(text: &str, motion_mode: MotionMode) -> Vec<Span<'static>> {
    shimmer_text_at(text, motion_mode, tui_clock::clock::now_ms())
}

/// Create shimmer spans for text with time-based color animation.
///
/// The highlight band keeps a fixed fractional width while the text grows,
/// so allocation stays constant: one span per segment (pre-dim, rise,
/// peak, fall, post-dim — at most five) instead of one span per character.
/// Sweep period and bandwidth match the previous per-character version.
fn shimmer_spans_at(text: &str, now_ms: u64) -> Vec<Span<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let len = graphemes.len();
    if len == 0 {
        return Vec::new();
    }

    let sweep = (now_ms % SHIMMER_PERIOD_MS) as f64 / SHIMMER_PERIOD_MS as f64;
    let center = sweep * len as f64;
    let half = (len as f64 * SHIMMER_HALF_WIDTH_FRAC).max(1.0);
    let start = center - half;
    let end = center + half;
    let peak = center.round() as usize;

    let dim = ratatui::style::Style::default().fg(ratatui::style::Color::Rgb(0x60, 0x60, 0x60));
    let edge = ratatui::style::Style::default().fg(ratatui::style::Color::Rgb(0xC0, 0xC0, 0xC0));
    let hot = ratatui::style::Style::default().fg(ratatui::style::Color::Rgb(0xFF, 0xFF, 0xFF));

    // Segment boundaries in grapheme indices, clamped to the text.
    let b0 = start.floor().max(0.0) as usize;
    let b1 = peak.min(len);
    let b2 = (peak + 1).min(len);
    let b3 = end.ceil().clamp(0.0, len as f64) as usize;

    let collect = |from: usize, to: usize| -> String {
        let (from, to) = (from.min(len), to.min(len));
        if from >= to {
            return String::new();
        }
        graphemes[from..to].concat()
    };

    let mut spans = Vec::with_capacity(5);
    let pre = collect(0, b0);
    if !pre.is_empty() {
        spans.push(Span::styled(pre, dim));
    }
    let rise = collect(b0, b1);
    if !rise.is_empty() {
        spans.push(Span::styled(rise, edge));
    }
    let top = collect(b1, b2);
    if !top.is_empty() {
        spans.push(Span::styled(top, hot));
    }
    let fall = collect(b2, b3.max(b2));
    if !fall.is_empty() {
        spans.push(Span::styled(fall, edge));
    }
    let post = collect(b3.max(b2), len);
    if !post.is_empty() {
        spans.push(Span::styled(post, dim));
    }
    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }
    spans
}

/// Linear interpolation between two values.
#[cfg(test)]
fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t).clamp(0.0, 255.0) as u8
}

/// Injected environment snapshot for motion detection. Upper layers build
/// this from `std::env` and desktop settings; the pure
/// [`detect_motion_preference_with`] consumes only these values, so unit
/// tests never touch process state.
#[derive(Debug, Clone, Default)]
pub struct MotionEnv {
    /// `NO_COLOR` present.
    pub no_color: bool,
    /// `TERM` value.
    pub term: Option<String>,
    /// GNOME `enable-animations` gsettings result (`None` when unknown).
    pub gnome_animations: Option<bool>,
    /// macOS `reduceMotion` defaults result (`None` when unknown).
    pub macos_reduce_motion: Option<bool>,
}

impl MotionEnv {
    /// Capture the live process environment (thin upper-layer adapter).
    pub fn live() -> Self {
        Self {
            no_color: std::env::var("NO_COLOR").is_ok(),
            term: std::env::var("TERM").ok(),
            gnome_animations: gnome_animations_setting(),
            macos_reduce_motion: macos_reduce_motion_setting(),
        }
    }
}

fn gnome_animations_setting() -> Option<bool> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.trim() {
        "false" => Some(false),
        "true" => Some(true),
        _ => None,
    }
}

fn macos_reduce_motion_setting() -> Option<bool> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = std::process::Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim() == "1")
}

/// Pure motion preference over injected environment values.
pub fn detect_motion_preference_with(env: &MotionEnv) -> MotionMode {
    if env.macos_reduce_motion == Some(true) {
        return MotionMode::Reduced;
    }
    if env.gnome_animations == Some(false) {
        return MotionMode::Reduced;
    }
    if env.no_color {
        return MotionMode::Reduced;
    }
    if let Some(term) = env.term.as_deref() {
        if term == "dumb" || term == "linux" {
            return MotionMode::Reduced;
        }
    }
    MotionMode::Animated
}

/// Pure truecolor support over injected environment values.
pub fn supports_truecolor_with(
    colorterm: Option<&str>,
    term_program: Option<&str>,
    term: Option<&str>,
) -> bool {
    if colorterm
        .map(|v| v.eq_ignore_ascii_case("truecolor"))
        .unwrap_or(false)
    {
        return true;
    }
    if let Some(program) = term_program {
        let program = program.to_ascii_lowercase();
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
    if let Some(term) = term {
        if term.contains("256color") || term.contains("truecolor") {
            return true;
        }
    }
    false
}

/// Detect reduced motion preference from the live environment. Thin
/// adapter over [`detect_motion_preference_with`]; policy code should prefer
/// injecting [`MotionEnv`] so behavior stays testable.
pub fn detect_motion_preference() -> MotionMode {
    detect_motion_preference_with(&MotionEnv::live())
}

/// Check if the terminal supports truecolor (24-bit color). Thin adapter
/// over [`supports_truecolor_with`].
pub fn supports_truecolor() -> bool {
    supports_truecolor_with(
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    )
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

    fn span_texts(spans: &[ratatui::text::Span<'_>]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

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
        let indicator =
            activity_indicator_at(0, 0, MotionMode::Animated, ReducedMotionIndicator::Hidden);
        assert!(indicator.is_some());
        let span = indicator.expect("animated indicator renders");
        assert!(!span.content.is_empty());
    }

    #[test]
    fn activity_indicator_steps_through_lookup_frames() {
        let at = |now| {
            activity_indicator_at(0, now, MotionMode::Animated, ReducedMotionIndicator::Hidden)
                .expect("animated indicator renders")
                .content
                .into_owned()
        };
        assert_eq!(at(0), BRAILLE_FRAMES[0]);
        assert_eq!(at(200), BRAILLE_FRAMES[1]);
        assert_eq!(at(800), BRAILLE_FRAMES[0]);
    }

    #[test]
    fn activity_indicator_reduced_mode_hidden() {
        let indicator =
            activity_indicator_at(0, 500, MotionMode::Reduced, ReducedMotionIndicator::Hidden);
        assert!(indicator.is_none());
    }

    #[test]
    fn activity_indicator_reduced_mode_bullet() {
        let indicator = activity_indicator_at(
            0,
            500,
            MotionMode::Reduced,
            ReducedMotionIndicator::StaticBullet,
        );
        assert!(indicator.is_some());
        let span = indicator.expect("reduced bullet renders");
        assert_eq!(span.content.as_ref(), "●");
    }

    #[test]
    fn shimmer_text_is_constant_allocation_and_lossless() {
        let long: String = "x".repeat(500);
        let spans = shimmer_text_at(&long, MotionMode::Animated, 750);
        assert!(
            spans.len() <= 5,
            "segments stay constant, got {}",
            spans.len()
        );
        let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, long);
    }

    #[test]
    fn shimmer_text_sweep_is_deterministic() {
        let first = shimmer_text_at("hello world", MotionMode::Animated, 100);
        let second = shimmer_text_at("hello world", MotionMode::Animated, 100);
        assert_eq!(span_texts(&first), span_texts(&second));
        let moved = shimmer_text_at("hello world", MotionMode::Animated, 1_100);
        let joined: String = moved.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(joined, "hello world");
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
