//! Animation system for TUI components.
//!
//! This module provides animation primitives for core UI components.
//! Animations are optional and can be disabled via configuration.
//! Supports three animation modes: full animation, reduced motion (accessibility),
//! and static (no animation).

use std::time::Duration;

/// Spinner frame glyphs indexed by [`AnimationController::spinner_frame_at`].
pub const SPINNER_CHARS: [&str; 8] = ["|", "/", "-", "\\", "|", "/", "-", "\\"];
/// Pulse frame glyphs indexed by [`AnimationController::pulse_frame_at`].
pub const PULSE_CHARS: [&str; 4] = ["●", "◉", "●", "○"];
/// Bounce frame glyphs indexed by [`AnimationController::bounce_frame_at`].
pub const BOUNCE_CHARS: [&str; 3] = ["_", "-", "="];
/// Fade opacities indexed by [`AnimationController::fade_frame_at`].
pub const FADE_OPACITIES: [f32; 5] = [0.2, 0.4, 0.6, 0.8, 1.0];

/// Full spinner cycle in milliseconds (one step per eighth of it).
const SPINNER_CYCLE_MS: u64 = 800;
/// Full pulse cycle in milliseconds.
const PULSE_CYCLE_MS: u64 = 400;
/// Full bounce cycle in milliseconds.
const BOUNCE_CYCLE_MS: u64 = 300;
/// Full fade cycle in milliseconds.
const FADE_CYCLE_MS: u64 = 500;

/// Animation mode for controlling animation behavior.
///
/// This enum controls how animations are rendered throughout the TUI:
/// - `Animated`: Full animation support with all effects enabled.
/// - `Reduced`: Reduced motion mode for accessibility; only essential animations.
/// - `Static`: No animations; all components render in their final state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationMode {
    /// Full animation support.
    #[default]
    Animated,
    /// Reduced motion for accessibility (respects system preferences).
    Reduced,
    /// No animations; static rendering only.
    Static,
}

impl AnimationMode {
    /// Create mode from a boolean: `true` = Animated, `false` = Static.
    pub fn from_enabled(enabled: bool) -> Self {
        if enabled {
            Self::Animated
        } else {
            Self::Static
        }
    }

    /// Returns true if animations should run (Animated mode).
    pub fn should_animate(self) -> bool {
        self == Self::Animated
    }

    /// Returns true if reduced motion is requested (Reduced or Static mode).
    pub fn is_reduced(self) -> bool {
        self != Self::Animated
    }
}

/// Animation state for a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationState {
    /// No animation.
    None,
    /// Animation is playing.
    Playing,
    /// Animation is paused.
    Paused,
}

/// Animation type for different component states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationType {
    /// Spinner animation for loading states.
    Spinner,
    /// Pulse animation for active states.
    Pulse,
    /// Bounce animation for emphasis.
    Bounce,
    /// Fade animation for transitions.
    Fade,
}

/// Animation configuration.
#[derive(Debug, Clone)]
pub struct AnimationConfig {
    /// Whether animations are enabled.
    pub enabled: bool,
    /// Animation speed multiplier (1.0 = normal).
    pub speed: f32,
    /// Frame duration for animations.
    pub frame_duration: Duration,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 1.0,
            frame_duration: Duration::from_millis(100), // 10 FPS
        }
    }
}

/// Trait for components that support animation ticks.
///
/// Components implementing this trait can receive animation tick notifications
/// to update their visual state. The `animation_tick()` method is called
/// periodically to advance animation frames.
pub trait Animatable {
    /// Advance the animation by one tick.
    ///
    /// Called periodically to update animation state. Implementations should
    /// update any time-dependent visual elements and invalidate caches as needed.
    fn animation_tick(&mut self);

    /// Returns true if this component should animate.
    ///
    /// Components can override this to disable animation based on their
    /// current state or configuration.
    fn should_animate(&self) -> bool {
        true
    }
}

/// Animation controller for managing multiple animations.
///
/// Frame math never reads the wall clock directly: construction stamps the
/// injectable [`tui_clock::clock::now_ms`] and every `*_frame` query routes
/// through its `*_at` variant, so tests assert exact frames by advancing the
/// simulated clock.
#[derive(Debug)]
pub struct AnimationController {
    config: AnimationConfig,
    mode: AnimationMode,
    start_ms: u64,
}

impl AnimationController {
    /// Create a new animation controller.
    pub fn new(config: AnimationConfig) -> Self {
        let enabled = config.enabled;
        Self {
            config,
            mode: AnimationMode::from_enabled(enabled),
            start_ms: tui_clock::clock::now_ms(),
        }
    }

    /// Create a controller with default configuration.
    pub fn default_enabled() -> Self {
        Self::new(AnimationConfig::default())
    }

    /// Create a controller with animations disabled.
    pub fn disabled() -> Self {
        Self::new(AnimationConfig {
            enabled: false,
            ..Default::default()
        })
    }

    /// Create a controller with a specific animation mode.
    pub fn with_mode(mode: AnimationMode) -> Self {
        Self {
            config: AnimationConfig {
                enabled: mode.should_animate(),
                ..Default::default()
            },
            mode,
            start_ms: tui_clock::clock::now_ms(),
        }
    }

    /// Test hook: controller stamped at an explicit clock value.
    pub fn with_start_ms(config: AnimationConfig, start_ms: u64) -> Self {
        let enabled = config.enabled;
        Self {
            config,
            mode: AnimationMode::from_enabled(enabled),
            start_ms,
        }
    }

    /// Set the animation mode.
    pub fn set_mode(&mut self, mode: AnimationMode) {
        self.mode = mode;
        self.config.enabled = mode.should_animate();
    }

    /// Get the current animation mode.
    pub fn mode(&self) -> AnimationMode {
        self.mode
    }

    /// Get the current frame index for a spinner animation.
    pub fn spinner_frame(&self) -> usize {
        self.spinner_frame_at(tui_clock::clock::now_ms())
    }

    /// Spinner frame for an explicit clock value (deterministic).
    pub fn spinner_frame_at(&self, now_ms: u64) -> usize {
        self.frame_at(now_ms, SPINNER_CYCLE_MS, SPINNER_CHARS.len())
    }

    /// Get the current frame index for a pulse animation.
    pub fn pulse_frame(&self) -> usize {
        self.pulse_frame_at(tui_clock::clock::now_ms())
    }

    /// Pulse frame for an explicit clock value (deterministic).
    pub fn pulse_frame_at(&self, now_ms: u64) -> usize {
        self.frame_at(now_ms, PULSE_CYCLE_MS, PULSE_CHARS.len())
    }

    /// Get the current frame index for a bounce animation.
    pub fn bounce_frame(&self) -> usize {
        self.bounce_frame_at(tui_clock::clock::now_ms())
    }

    /// Bounce frame for an explicit clock value (deterministic).
    pub fn bounce_frame_at(&self, now_ms: u64) -> usize {
        self.frame_at(now_ms, BOUNCE_CYCLE_MS, BOUNCE_CHARS.len())
    }

    /// Get the current frame index for a fade animation.
    pub fn fade_frame(&self) -> usize {
        self.fade_frame_at(tui_clock::clock::now_ms())
    }

    /// Fade frame for an explicit clock value (deterministic).
    pub fn fade_frame_at(&self, now_ms: u64) -> usize {
        self.frame_at(now_ms, FADE_CYCLE_MS, FADE_OPACITIES.len())
    }

    /// Shared cycle math: whole cycles elapsed since start, sped up, modulo
    /// the frame count. Matches the previous per-method computation.
    fn frame_at(&self, now_ms: u64, cycle_ms: u64, frame_count: usize) -> usize {
        if !self.config.enabled || frame_count == 0 {
            return 0;
        }
        let elapsed = now_ms.saturating_sub(self.start_ms);
        let cycle = (f64::from(cycle_ms as u32) / f64::from(self.config.speed.max(f32::EPSILON)))
            .round() as u64;
        if cycle == 0 {
            return 0;
        }
        (elapsed / cycle.max(1)) as usize % frame_count
    }

    /// Get the spinner character for the current frame.
    pub fn spinner_char(&self) -> &'static str {
        self.spinner_char_at(tui_clock::clock::now_ms())
    }

    /// Spinner glyph for an explicit clock value (deterministic).
    pub fn spinner_char_at(&self, now_ms: u64) -> &'static str {
        SPINNER_CHARS[self.spinner_frame_at(now_ms) % SPINNER_CHARS.len()]
    }

    /// Get the pulse character for the current frame.
    pub fn pulse_char(&self) -> &'static str {
        self.pulse_char_at(tui_clock::clock::now_ms())
    }

    /// Pulse glyph for an explicit clock value (deterministic).
    pub fn pulse_char_at(&self, now_ms: u64) -> &'static str {
        PULSE_CHARS[self.pulse_frame_at(now_ms) % PULSE_CHARS.len()]
    }

    /// Get the bounce character for the current frame.
    pub fn bounce_char(&self) -> &'static str {
        self.bounce_char_at(tui_clock::clock::now_ms())
    }

    /// Bounce glyph for an explicit clock value (deterministic).
    pub fn bounce_char_at(&self, now_ms: u64) -> &'static str {
        BOUNCE_CHARS[self.bounce_frame_at(now_ms) % BOUNCE_CHARS.len()]
    }

    /// Get the fade opacity for the current frame (0.0 to 1.0).
    pub fn fade_opacity(&self) -> f32 {
        self.fade_opacity_at(tui_clock::clock::now_ms())
    }

    /// Fade opacity for an explicit clock value (deterministic).
    pub fn fade_opacity_at(&self, now_ms: u64) -> f32 {
        FADE_OPACITIES[self.fade_frame_at(now_ms) % FADE_OPACITIES.len()]
    }

    /// Check if animations are enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the animation configuration.
    pub fn config(&self) -> &AnimationConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_mode_from_enabled() {
        assert_eq!(AnimationMode::from_enabled(true), AnimationMode::Animated);
        assert_eq!(AnimationMode::from_enabled(false), AnimationMode::Static);
    }

    #[test]
    fn animation_mode_should_animate() {
        assert!(AnimationMode::Animated.should_animate());
        assert!(!AnimationMode::Reduced.should_animate());
        assert!(!AnimationMode::Static.should_animate());
    }

    #[test]
    fn animation_mode_is_reduced() {
        assert!(!AnimationMode::Animated.is_reduced());
        assert!(AnimationMode::Reduced.is_reduced());
        assert!(AnimationMode::Static.is_reduced());
    }

    #[test]
    fn controller_with_mode() {
        let controller = AnimationController::with_mode(AnimationMode::Reduced);
        assert_eq!(controller.mode(), AnimationMode::Reduced);
        assert!(!controller.is_enabled());
        assert_eq!(controller.spinner_frame(), 0);
    }

    #[test]
    fn controller_set_mode() {
        let mut controller = AnimationController::default_enabled();
        assert!(controller.is_enabled());

        controller.set_mode(AnimationMode::Static);
        assert_eq!(controller.mode(), AnimationMode::Static);
        assert!(!controller.is_enabled());
        assert_eq!(controller.spinner_frame(), 0);

        controller.set_mode(AnimationMode::Animated);
        assert!(controller.is_enabled());
    }

    #[test]
    fn spinner_frame_cycles() {
        let controller = AnimationController::default_enabled();
        // The spinner should cycle through 0-7
        for _ in 0..100 {
            let frame = controller.spinner_frame();
            assert!(frame < 8, "spinner frame should be 0-7, got {}", frame);
        }
    }

    #[test]
    fn pulse_frame_cycles() {
        let controller = AnimationController::default_enabled();
        for _ in 0..100 {
            let frame = controller.pulse_frame();
            assert!(frame < 4, "pulse frame should be 0-3, got {}", frame);
        }
    }

    #[test]
    fn bounce_frame_cycles() {
        let controller = AnimationController::default_enabled();
        for _ in 0..100 {
            let frame = controller.bounce_frame();
            assert!(frame < 3, "bounce frame should be 0-2, got {}", frame);
        }
    }

    #[test]
    fn fade_frame_cycles() {
        let controller = AnimationController::default_enabled();
        for _ in 0..100 {
            let frame = controller.fade_frame();
            assert!(frame < 5, "fade frame should be 0-4, got {}", frame);
        }
    }

    #[test]
    fn disabled_controller_returns_zero() {
        let controller = AnimationController::disabled();
        assert_eq!(controller.spinner_frame(), 0);
        assert_eq!(controller.pulse_frame(), 0);
        assert_eq!(controller.bounce_frame(), 0);
        assert_eq!(controller.fade_frame(), 0);
    }

    #[test]
    fn spinner_chars_are_valid() {
        let controller = AnimationController::default_enabled();
        let valid_chars = ["|", "/", "-", "\\"];
        for _ in 0..100 {
            let ch = controller.spinner_char();
            assert!(valid_chars.contains(&ch), "invalid spinner char: {}", ch);
        }
    }

    #[test]
    fn frames_are_deterministic_for_an_explicit_clock() {
        let controller = AnimationController::with_start_ms(AnimationConfig::default(), 1_000);
        assert_eq!(controller.spinner_frame_at(1_000), 0);
        assert_eq!(controller.spinner_frame_at(1_800), 1);
        assert_eq!(controller.spinner_char_at(1_000), "|");
        assert_eq!(controller.spinner_char_at(1_800), "/");
        assert_eq!(controller.pulse_char_at(1_000), PULSE_CHARS[0]);
        assert_eq!(controller.bounce_char_at(1_000), BOUNCE_CHARS[0]);
        assert_eq!(controller.fade_opacity_at(1_000), FADE_OPACITIES[0]);
        // A full cycle wraps back to the first frame.
        assert_eq!(controller.spinner_frame_at(1_000 + 8 * 800), 0);
        assert_eq!(controller.fade_frame_at(1_000 + 5 * 500), 0);
    }

    #[test]
    fn fade_opacity_in_range() {
        let controller = AnimationController::default_enabled();
        for _ in 0..100 {
            let opacity = controller.fade_opacity();
            assert!(
                (0.0..=1.0).contains(&opacity),
                "fade opacity should be 0.0-1.0, got {}",
                opacity
            );
        }
    }
}
