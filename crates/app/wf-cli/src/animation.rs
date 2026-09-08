//! Animation system for TUI components.
//!
//! This module provides animation primitives for core UI components.
//! Animations are optional and can be disabled via configuration.

use std::time::{Duration, Instant};

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

/// Animation controller for managing multiple animations.
#[derive(Debug)]
pub struct AnimationController {
    config: AnimationConfig,
    start_time: Instant,
}

impl AnimationController {
    /// Create a new animation controller.
    pub fn new(config: AnimationConfig) -> Self {
        Self {
            config,
            start_time: Instant::now(),
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

    /// Get the current frame index for a spinner animation.
    pub fn spinner_frame(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }
        let elapsed = self.start_time.elapsed();
        let frames_per_cycle = 8; // 8 spinner frames
        let cycle_duration = Duration::from_millis(800); // 800ms per cycle
        let adjusted_duration =
            Duration::from_secs_f32(cycle_duration.as_secs_f32() / self.config.speed);
        let cycles = elapsed.as_millis() / adjusted_duration.as_millis();
        (cycles as usize) % frames_per_cycle
    }

    /// Get the current frame index for a pulse animation.
    pub fn pulse_frame(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }
        let elapsed = self.start_time.elapsed();
        let frames_per_cycle = 4; // 4 pulse frames
        let cycle_duration = Duration::from_millis(400); // 400ms per cycle
        let adjusted_duration =
            Duration::from_secs_f32(cycle_duration.as_secs_f32() / self.config.speed);
        let cycles = elapsed.as_millis() / adjusted_duration.as_millis();
        (cycles as usize) % frames_per_cycle
    }

    /// Get the current frame index for a bounce animation.
    pub fn bounce_frame(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }
        let elapsed = self.start_time.elapsed();
        let frames_per_cycle = 3; // 3 bounce frames
        let cycle_duration = Duration::from_millis(300); // 300ms per cycle
        let adjusted_duration =
            Duration::from_secs_f32(cycle_duration.as_secs_f32() / self.config.speed);
        let cycles = elapsed.as_millis() / adjusted_duration.as_millis();
        (cycles as usize) % frames_per_cycle
    }

    /// Get the current frame index for a fade animation.
    pub fn fade_frame(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }
        let elapsed = self.start_time.elapsed();
        let frames_per_cycle = 5; // 5 fade frames
        let cycle_duration = Duration::from_millis(500); // 500ms per cycle
        let adjusted_duration =
            Duration::from_secs_f32(cycle_duration.as_secs_f32() / self.config.speed);
        let cycles = elapsed.as_millis() / adjusted_duration.as_millis();
        (cycles as usize) % frames_per_cycle
    }

    /// Get the spinner character for the current frame.
    pub fn spinner_char(&self) -> &'static str {
        match self.spinner_frame() {
            0 => "|",
            1 => "/",
            2 => "-",
            3 => "\\",
            4 => "|",
            5 => "/",
            6 => "-",
            7 => "\\",
            _ => "|",
        }
    }

    /// Get the pulse character for the current frame.
    pub fn pulse_char(&self) -> &'static str {
        match self.pulse_frame() {
            0 => "●",
            1 => "◉",
            2 => "●",
            3 => "○",
            _ => "●",
        }
    }

    /// Get the bounce character for the current frame.
    pub fn bounce_char(&self) -> &'static str {
        match self.bounce_frame() {
            0 => "_",
            1 => "-",
            2 => "=",
            _ => "_",
        }
    }

    /// Get the fade opacity for the current frame (0.0 to 1.0).
    pub fn fade_opacity(&self) -> f32 {
        match self.fade_frame() {
            0 => 0.2,
            1 => 0.4,
            2 => 0.6,
            3 => 0.8,
            4 => 1.0,
            _ => 0.2,
        }
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
            assert!(
                valid_chars.contains(&ch),
                "invalid spinner char: {}",
                ch
            );
        }
    }

    #[test]
    fn fade_opacity_in_range() {
        let controller = AnimationController::default_enabled();
        for _ in 0..100 {
            let opacity = controller.fade_opacity();
            assert!(
                opacity >= 0.0 && opacity <= 1.0,
                "fade opacity should be 0.0-1.0, got {}",
                opacity
            );
        }
    }
}