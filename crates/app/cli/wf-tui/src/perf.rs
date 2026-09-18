//! Capability-driven performance tiers and the runtime policy.
//!
//! [`SystemProfile`] captures the environment signals (load, memory, ssh,
//! terminal identity), [`select_tier`] maps them onto a
//! [`PerformanceTier`], and [`TuiPerfPolicy`] maps the tier onto concrete
//! runtime behavior: redraw/animation frame rates, decorative animation and
//! input-capability switches, and synchronized-output eligibility. The
//! policy is built once at startup and injected into the render and event
//! layers; [`PerformanceTier::marker`] surfaces the active tier in the
//! status line so downgrades stay visible.

use crate::capabilities::TerminalCapabilities;

/// Runtime capability tier: full keeps every effect, reduced drops
/// decoration, minimal additionally drops costly input capabilities and
/// uses simplified rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerformanceTier {
    /// All animations, input capabilities and synchronized output.
    #[default]
    Full,
    /// No decorative animations, lower animation rate, spinners stay.
    Reduced,
    /// No animations, costly input capabilities off, simplified rendering.
    Minimal,
}

impl PerformanceTier {
    /// Short status-line marker for the tier.
    pub fn marker(self) -> &'static str {
        match self {
            PerformanceTier::Full => "perf:full",
            PerformanceTier::Reduced => "perf:reduced",
            PerformanceTier::Minimal => "perf:minimal",
        }
    }
}

/// Environment signals feeding tier selection.
#[derive(Debug, Clone, Default)]
pub struct SystemProfile {
    /// One-minute load average, if known.
    pub load_avg_1m: Option<f64>,
    /// Logical CPU count, if known.
    pub cpu_count: Option<usize>,
    /// Available memory in MiB, if known.
    pub available_memory_mb: Option<u64>,
    /// Whether the session runs over ssh.
    pub is_ssh: bool,
    /// Terminal identity string (`TERM_PROGRAM` / `TERM`), if known.
    pub terminal: Option<String>,
}

impl SystemProfile {
    /// Probe the local environment (pure std reads, never fails).
    pub fn detect() -> Self {
        let is_ssh = std::env::var("SSH_CONNECTION")
            .or_else(|_| std::env::var("SSH_CLIENT"))
            .is_ok();
        let terminal = std::env::var("TERM_PROGRAM")
            .or_else(|_| std::env::var("TERM"))
            .ok();
        Self {
            load_avg_1m: load_average_1m(),
            cpu_count: std::thread::available_parallelism().ok().map(|n| n.get()),
            available_memory_mb: None,
            is_ssh,
            terminal,
        }
    }

    /// Load per CPU, when both signals are known.
    pub fn load_ratio(&self) -> Option<f64> {
        match (self.load_avg_1m, self.cpu_count) {
            (Some(load), Some(cpus)) if cpus > 0 => Some(load / cpus as f64),
            _ => None,
        }
    }

    /// True for terminals whose glyph cache corrupts under heavy per-cell
    /// color animation; those force glyph-safe rendering.
    pub fn fragile_glyph_cache(&self) -> bool {
        let term = self.terminal.as_deref().unwrap_or("").to_ascii_lowercase();
        term.contains("tmux") || term.contains("screen") || std::env::var("TMUX").is_ok()
    }
}

/// Best-effort 1-minute load average from `/proc/loadavg`.
fn load_average_1m() -> Option<f64> {
    let raw = std::fs::read_to_string("/proc/loadavg").ok()?;
    raw.split_whitespace().next()?.parse::<f64>().ok()
}

/// Concrete runtime behavior for a tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiPerfPolicy {
    /// Cap for full frames per second.
    pub redraw_fps: u64,
    /// Cap for animation-only frames per second.
    pub animation_fps: u64,
    /// Decorative animations (shimmer, pulses) allowed.
    pub decorative_animations: bool,
    /// Whether the essential spinner stays on in reduced tiers.
    pub keep_spinner: bool,
    /// Focus-change event reporting enabled.
    pub enable_focus_change: bool,
    /// Mouse capture enabled.
    pub enable_mouse_capture: bool,
    /// Kitty keyboard enhancement enabled.
    pub enable_keyboard_enhancement: bool,
    /// Synchronized output eligible (still requires terminal support).
    pub sync_output: bool,
    /// Simplified transcript rendering (fewer styles, no shimmer).
    pub simplified_render: bool,
}

impl TuiPerfPolicy {
    /// Policy for a tier.
    pub fn for_tier(tier: PerformanceTier) -> Self {
        match tier {
            PerformanceTier::Full => Self {
                redraw_fps: 120,
                animation_fps: 10,
                decorative_animations: true,
                keep_spinner: true,
                enable_focus_change: true,
                enable_mouse_capture: true,
                enable_keyboard_enhancement: true,
                sync_output: true,
                simplified_render: false,
            },
            PerformanceTier::Reduced => Self {
                redraw_fps: 60,
                animation_fps: 5,
                decorative_animations: false,
                keep_spinner: true,
                enable_focus_change: true,
                enable_mouse_capture: false,
                enable_keyboard_enhancement: true,
                sync_output: true,
                simplified_render: false,
            },
            PerformanceTier::Minimal => Self {
                redraw_fps: 30,
                animation_fps: 2,
                decorative_animations: false,
                keep_spinner: false,
                enable_focus_change: false,
                enable_mouse_capture: false,
                enable_keyboard_enhancement: false,
                sync_output: false,
                simplified_render: true,
            },
        }
    }

    /// Minimum full-frame interval in milliseconds.
    pub fn redraw_interval_ms(self) -> u64 {
        1_000 / self.redraw_fps.max(1)
    }

    /// Minimum animation-frame interval in milliseconds.
    pub fn animation_interval_ms(self) -> u64 {
        1_000 / self.animation_fps.max(1)
    }
}

/// Select a tier from the system profile and probed capabilities.
/// Conservative by design: only decorative abilities degrade, core input
/// stays enabled until the minimal tier.
pub fn select_tier(profile: &SystemProfile, caps: &TerminalCapabilities) -> PerformanceTier {
    if profile.fragile_glyph_cache() {
        return PerformanceTier::Reduced;
    }
    let pressured = profile.load_ratio().is_some_and(|r| r > 2.0);
    let sparse_color = matches!(
        caps.color_depth,
        crate::capabilities::ColorDepth::Monochrome | crate::capabilities::ColorDepth::Ansi16
    );
    if profile.is_ssh && (pressured || sparse_color) {
        return PerformanceTier::Minimal;
    }
    if pressured || profile.is_ssh || sparse_color {
        return PerformanceTier::Reduced;
    }
    PerformanceTier::Full
}

/// Whether a frame with `scope` should be wrapped in synchronized output.
/// Only full and bottom frames qualify (animation frames tick too often to
/// pay the wrapping cost), and the terminal must advertise support.
pub fn should_sync_output(
    policy: TuiPerfPolicy,
    terminal_supports_sync: bool,
    scope: crate::redraw::RedrawScope,
) -> bool {
    if !policy.sync_output || !terminal_supports_sync {
        return false;
    }
    matches!(
        scope,
        crate::redraw::RedrawScope::Full | crate::redraw::RedrawScope::BottomOnly
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::ColorDepth;

    fn caps_with(depth: ColorDepth) -> TerminalCapabilities {
        TerminalCapabilities {
            color_depth: depth,
            ..TerminalCapabilities::default()
        }
    }

    #[test]
    fn full_tier_keeps_everything() {
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        assert!(policy.decorative_animations);
        assert!(policy.enable_mouse_capture);
        assert!(policy.sync_output);
        assert!(!policy.simplified_render);
        assert_eq!(policy.redraw_interval_ms(), 8);
    }

    #[test]
    fn reduced_tier_keeps_spinner_but_drops_decoration() {
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Reduced);
        assert!(!policy.decorative_animations);
        assert!(policy.keep_spinner);
        assert!(!policy.enable_mouse_capture);
        assert!(policy.enable_keyboard_enhancement);
    }

    #[test]
    fn minimal_tier_drops_input_and_simplifies() {
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Minimal);
        assert!(!policy.enable_focus_change);
        assert!(!policy.enable_keyboard_enhancement);
        assert!(!policy.sync_output);
        assert!(policy.simplified_render);
    }

    #[test]
    fn healthy_local_terminal_stays_full() {
        let profile = SystemProfile::default();
        assert_eq!(
            select_tier(&profile, &caps_with(ColorDepth::TrueColor)),
            PerformanceTier::Full
        );
    }

    #[test]
    fn overloaded_host_reduces() {
        let profile = SystemProfile {
            load_avg_1m: Some(16.0),
            cpu_count: Some(4),
            ..SystemProfile::default()
        };
        assert_eq!(
            select_tier(&profile, &caps_with(ColorDepth::TrueColor)),
            PerformanceTier::Reduced
        );
    }

    #[test]
    fn fragile_glyph_cache_forces_reduced() {
        let profile = SystemProfile {
            terminal: Some("tmux-256color".to_string()),
            ..SystemProfile::default()
        };
        assert!(profile.fragile_glyph_cache());
        assert_eq!(
            select_tier(&profile, &caps_with(ColorDepth::TrueColor)),
            PerformanceTier::Reduced
        );
    }

    #[test]
    fn sync_output_skips_animation_frames() {
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        assert!(should_sync_output(
            policy,
            true,
            crate::redraw::RedrawScope::Full
        ));
        assert!(should_sync_output(
            policy,
            true,
            crate::redraw::RedrawScope::BottomOnly
        ));
        assert!(!should_sync_output(
            policy,
            true,
            crate::redraw::RedrawScope::AnimationOnly
        ));
        assert!(!should_sync_output(
            policy,
            false,
            crate::redraw::RedrawScope::Full
        ));
    }
}
