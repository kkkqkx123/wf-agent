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

use tui_terminal::capabilities::TerminalCapabilities;

/// Runtime capability tier: full keeps every effect, reduced drops
/// decoration, minimal additionally drops costly input capabilities and
/// uses simplified rendering.
///
/// This is the TUI capability tier, unrelated to the analysis grading in
/// `wf_api::analysis` which shares only the name.
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
        if fragile_glyph_terminal(self.terminal.as_deref()) {
            return true;
        }
        std::env::var("TMUX").is_ok()
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
    /// Mouse capture enabled (explicit user opt-in only).
    pub enable_mouse_capture: bool,
    /// Kitty keyboard enhancement enabled.
    pub enable_keyboard_enhancement: bool,
    /// Alternate scroll (1007) enabled: the wheel arrives as up/down keys
    /// without capturing the mouse, so native selection keeps working.
    pub enable_alternate_scroll: bool,
    /// Synchronized output eligible (still requires terminal support).
    pub sync_output: bool,
    /// Simplified transcript rendering (fewer styles, no shimmer).
    pub simplified_render: bool,
}

impl TuiPerfPolicy {
    /// Policy for a tier. The tier only clamps frame rates, animation rates
    /// and decorative animation; input capabilities default to on (except
    /// mouse capture, which defaults to off pending explicit opt-in) and
    /// are refined by [`TuiPerfPolicy::apply_input_gates`].
    pub fn for_tier(tier: PerformanceTier) -> Self {
        match tier {
            PerformanceTier::Full => Self {
                redraw_fps: 120,
                animation_fps: 10,
                decorative_animations: true,
                keep_spinner: true,
                enable_focus_change: true,
                enable_mouse_capture: false,
                enable_keyboard_enhancement: true,
                enable_alternate_scroll: true,
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
                enable_alternate_scroll: true,
                sync_output: true,
                simplified_render: false,
            },
            PerformanceTier::Minimal => Self {
                redraw_fps: 30,
                animation_fps: 2,
                decorative_animations: false,
                keep_spinner: false,
                enable_focus_change: true,
                enable_mouse_capture: false,
                enable_keyboard_enhancement: true,
                enable_alternate_scroll: true,
                sync_output: false,
                simplified_render: true,
            },
        }
    }

    /// Refine the input-capability switches with the environment gates. The
    /// tier never turns inputs off by itself; each gate can only disable, so
    /// fixing a policy switch to off rolls the corresponding stage back.
    pub fn apply_input_gates(&mut self, gates: &InputGates) {
        self.enable_mouse_capture = gates.mouse_opt_in;
        self.enable_focus_change = gates.focus_supported && !gates.unreliable_focus_terminal;
        self.enable_keyboard_enhancement = gates.keyboard_supported
            && !gates.keyboard_env_disabled
            && !gates.blocked_keyboard_combination;
    }

    /// Minimum full-frame interval in milliseconds.
    pub fn redraw_interval_ms(self) -> u64 {
        1_000 / self.redraw_fps.max(1)
    }

    /// Minimum animation-frame interval in milliseconds. Zero fps (killed
    /// by the environment master switch) never becomes due.
    pub fn animation_interval_ms(self) -> u64 {
        if self.animation_fps == 0 {
            return u64::MAX;
        }
        1_000 / self.animation_fps.max(1)
    }

    /// Glyph-safe variant for fragile terminals: suppress decorative
    /// per-cell animation while keeping the essential spinner, so the GPU
    /// glyph atlas stays stable under color churn. Also clamps the frame
    /// rates to the reduced tier so full frames stop churning the atlas.
    pub fn glyph_safe(self) -> Self {
        Self {
            decorative_animations: false,
            simplified_render: true,
            redraw_fps: self.redraw_fps.min(60),
            animation_fps: self.animation_fps.min(5),
            ..self
        }
    }

    /// Apply the glyph-safe variant when `fragile` holds.
    pub fn apply_glyph_safety(&mut self, fragile: bool) {
        if fragile {
            *self = self.glyph_safe();
        }
    }

    /// Apply the environment master switches. `WF_TUI_DISABLE_ANIMATION` or
    /// `NO_ANIMATION` (truthy) turns off decorative animation and animation
    /// frames; `NO_COLOR` forces simplified rendering. Either switch is a
    /// total kill that never affects content correctness.
    pub fn apply_env_overrides(&mut self) {
        if animation_disabled_by_env() {
            self.decorative_animations = false;
            self.keep_spinner = false;
            self.animation_fps = 0;
        }
        if std::env::var("NO_COLOR").is_ok() {
            self.simplified_render = true;
            self.decorative_animations = false;
        }
    }
}

/// Input-capability gates feeding [`TuiPerfPolicy::apply_input_gates`].
/// Mouse comes from the user config, focus and keyboard default to on and
/// turn off on probe, environment or terminal-matrix evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputGates {
    /// User config opt-in for mouse capture (default off).
    pub mouse_opt_in: bool,
    /// Capability probe reports focus-event support.
    pub focus_supported: bool,
    /// Capability probe reports kitty keyboard enhancement support.
    pub keyboard_supported: bool,
    /// Environment master switch disabled keyboard enhancement.
    pub keyboard_env_disabled: bool,
    /// Terminal identity is on the unreliable-focus list.
    pub unreliable_focus_terminal: bool,
    /// Terminal combination known to misreport enhanced keys.
    pub blocked_keyboard_combination: bool,
}

impl InputGates {
    /// All capabilities on: unit-test baseline, never used in production.
    pub fn all_on() -> Self {
        Self {
            mouse_opt_in: true,
            focus_supported: true,
            keyboard_supported: true,
            keyboard_env_disabled: false,
            unreliable_focus_terminal: false,
            blocked_keyboard_combination: false,
        }
    }
}

/// Build the input gates from the live environment: explicit user opt-in
/// for the mouse, probe results for focus and keyboard, the environment
/// master switch and the unreliable-terminal matrix for the rest.
pub fn detect_input_gates(
    profile: &SystemProfile,
    caps: &TerminalCapabilities,
    mouse_opt_in: bool,
) -> InputGates {
    InputGates {
        mouse_opt_in,
        focus_supported: caps.focus_events,
        keyboard_supported: caps.kitty_keyboard,
        keyboard_env_disabled: tui_terminal::terminal::keyboard_enhancement_env_disabled(),
        unreliable_focus_terminal: unreliable_focus_terminal(profile.terminal.as_deref()),
        blocked_keyboard_combination: blocked_keyboard_combination(),
    }
}

/// True for terminal identities with a fragile glyph cache under per-cell
/// color churn: multiplexers plus known desktop integrated terminals whose
/// GPU atlas corrupts on heavy animation.
pub fn fragile_glyph_terminal(terminal: Option<&str>) -> bool {
    let term = terminal.unwrap_or("").to_ascii_lowercase();
    term.contains("tmux")
        || term.contains("screen")
        || term.contains("vscode")
        || term.contains("code-oss")
        || (term.contains("apple") && term.contains("terminal"))
        || term == "apple_terminal"
}

/// Fragile-glyph check consuming both the system profile string and the
/// probed terminal type, so multiplexers detected by either path force
/// glyph-safe rendering.
pub fn fragile_glyph_cache_with_caps(profile: &SystemProfile, caps: &TerminalCapabilities) -> bool {
    if profile.fragile_glyph_cache() {
        return true;
    }
    matches!(
        caps.terminal_type,
        tui_terminal::capabilities::TerminalType::Tmux
            | tui_terminal::capabilities::TerminalType::Screen
    )
}

/// Environment master switch for animation. Truthy `WF_TUI_DISABLE_ANIMATION`
/// or `NO_ANIMATION` disables every decorative and spinner frame.
pub const DISABLE_ANIMATION_ENV: &str = "WF_TUI_DISABLE_ANIMATION";
/// Secondary master switch honoring the common `NO_ANIMATION` convention.
pub const NO_ANIMATION_ENV: &str = "NO_ANIMATION";

/// True when the environment requests no animation.
pub fn animation_disabled_by_env() -> bool {
    parse_truthy_env(DISABLE_ANIMATION_ENV) || parse_truthy_env(NO_ANIMATION_ENV)
}

fn parse_truthy_env(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref().map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

/// True for terminal identities whose focus events are unreliable: the
/// Windows Terminal family stays off even when the probe reports support.
pub fn unreliable_focus_terminal(terminal: Option<&str>) -> bool {
    if std::env::var("WT_SESSION").is_ok() {
        return true;
    }
    let term = terminal.unwrap_or("").to_ascii_lowercase();
    term.contains("windows") || term.contains("mintty") || term.contains("conhost")
}

/// True for terminal combinations known to misreport enhanced keys: a WSL
/// shell hosted by the VS Code terminal hides the real terminal identity
/// from the Linux environment.
pub fn blocked_keyboard_combination() -> bool {
    is_wsl() && is_vscode_terminal()
}

/// True inside Windows Subsystem for Linux.
pub fn is_wsl() -> bool {
    if std::env::var("WSL_DISTRO_NAME").is_ok() || std::env::var("WSL_INTEROP").is_ok() {
        return true;
    }
    std::fs::read_to_string("/proc/version")
        .map(|version| version.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

/// True when the Linux-side terminal identity claims VS Code.
pub fn is_vscode_terminal() -> bool {
    std::env::var("TERM_PROGRAM")
        .map(|value| value.eq_ignore_ascii_case("vscode"))
        .unwrap_or(false)
}

/// Named synthetic environment for tests: builds terminal profiles without
/// touching process-wide variables, so glyph-cache and keyboard-matrix
/// behavior is reproducible in unit tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticSystemProfile {
    /// Plain local terminal.
    Native,
    /// WSL shell outside VS Code.
    Wsl,
    /// WSL shell hosted by the VS Code terminal (keyboard matrix hits).
    WslWindowsTerminal,
}

impl SyntheticSystemProfile {
    /// Terminal identity string for the named environment.
    pub fn terminal(&self) -> &'static str {
        match self {
            SyntheticSystemProfile::Native => "xterm-256color",
            SyntheticSystemProfile::Wsl => "xterm-256color",
            SyntheticSystemProfile::WslWindowsTerminal => "vscode",
        }
    }

    /// System profile for the named environment.
    pub fn profile(&self) -> SystemProfile {
        SystemProfile {
            terminal: Some(self.terminal().to_string()),
            ..SystemProfile::default()
        }
    }

    /// Whether the named environment misreports enhanced keys (the WSL +
    /// VS Code combination hides the real terminal identity).
    pub fn blocked_keyboard(&self) -> bool {
        matches!(self, SyntheticSystemProfile::WslWindowsTerminal)
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
        tui_terminal::capabilities::ColorDepth::Monochrome
            | tui_terminal::capabilities::ColorDepth::Ansi16
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
    use tui_terminal::capabilities::ColorDepth;

    fn caps_with(depth: ColorDepth) -> TerminalCapabilities {
        TerminalCapabilities {
            color_depth: depth,
            ..TerminalCapabilities::default()
        }
    }

    #[test]
    fn full_tier_keeps_everything_except_mouse_opt_in() {
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        assert!(policy.decorative_animations);
        assert!(policy.enable_focus_change);
        assert!(policy.enable_keyboard_enhancement);
        assert!(policy.enable_alternate_scroll);
        assert!(!policy.enable_mouse_capture);
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
        assert!(policy.enable_focus_change);
    }

    #[test]
    fn minimal_tier_only_clamps_rates_and_rendering() {
        // The tier never decides input capabilities: focus and keyboard
        // stay on until the environment gates turn them off.
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Minimal);
        assert!(policy.enable_focus_change);
        assert!(policy.enable_keyboard_enhancement);
        assert!(!policy.enable_mouse_capture);
        assert!(!policy.sync_output);
        assert!(policy.simplified_render);
    }

    #[test]
    fn input_gates_enable_everything_when_evidence_allows() {
        let mut policy = TuiPerfPolicy::for_tier(PerformanceTier::Minimal);
        policy.apply_input_gates(&InputGates::all_on());
        assert!(policy.enable_focus_change);
        assert!(policy.enable_mouse_capture);
        assert!(policy.enable_keyboard_enhancement);
    }

    #[test]
    fn mouse_needs_explicit_opt_in() {
        let mut policy = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        let mut gates = InputGates::all_on();
        gates.mouse_opt_in = false;
        policy.apply_input_gates(&gates);
        assert!(!policy.enable_mouse_capture);
        assert!(policy.enable_focus_change);
        assert!(policy.enable_keyboard_enhancement);
    }

    #[test]
    fn focus_turns_off_on_probe_or_unreliable_terminal() {
        let mut policy = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        let mut gates = InputGates::all_on();
        gates.focus_supported = false;
        policy.apply_input_gates(&gates);
        assert!(!policy.enable_focus_change);

        let mut gates = InputGates::all_on();
        gates.unreliable_focus_terminal = true;
        policy.apply_input_gates(&gates);
        assert!(!policy.enable_focus_change);
    }

    #[test]
    fn keyboard_triple_gate_any_denial_turns_it_off() {
        for blocked in [
            InputGates {
                keyboard_supported: false,
                ..InputGates::all_on()
            },
            InputGates {
                keyboard_env_disabled: true,
                ..InputGates::all_on()
            },
            InputGates {
                blocked_keyboard_combination: true,
                ..InputGates::all_on()
            },
        ] {
            let mut policy = TuiPerfPolicy::for_tier(PerformanceTier::Full);
            policy.apply_input_gates(&blocked);
            assert!(!policy.enable_keyboard_enhancement);
        }
    }

    #[test]
    fn unreliable_focus_matrix_covers_windows_terminal_family() {
        let saved = std::env::var("WT_SESSION").ok();
        std::env::remove_var("WT_SESSION");
        assert!(unreliable_focus_terminal(Some("windows-terminal")));
        assert!(unreliable_focus_terminal(Some("mintty-3.6")));
        assert!(unreliable_focus_terminal(Some("conhost")));
        assert!(!unreliable_focus_terminal(Some("kitty")));
        assert!(!unreliable_focus_terminal(Some("xterm-256color")));
        assert!(!unreliable_focus_terminal(None));
        if let Some(value) = saved {
            std::env::set_var("WT_SESSION", value);
        }
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

    #[test]
    fn synthetic_profiles_reproduce_without_env() {
        let native = SyntheticSystemProfile::Native;
        assert!(!native.profile().fragile_glyph_cache());
        assert!(!native.blocked_keyboard());
        let wsl = SyntheticSystemProfile::Wsl;
        assert!(!wsl.blocked_keyboard());
        let hosted = SyntheticSystemProfile::WslWindowsTerminal;
        assert!(hosted.blocked_keyboard());
        assert_eq!(hosted.terminal(), "vscode");
    }

    #[test]
    fn fragile_matrix_covers_desktop_terminals() {
        assert!(fragile_glyph_terminal(Some("vscode")));
        assert!(fragile_glyph_terminal(Some("tmux-256color")));
        assert!(fragile_glyph_terminal(Some("Apple_Terminal")));
        assert!(!fragile_glyph_terminal(Some("kitty")));
        assert!(!fragile_glyph_terminal(Some("xterm-256color")));
    }

    #[test]
    fn glyph_safe_keeps_spinner_but_drops_decoration() {
        let policy = TuiPerfPolicy::for_tier(PerformanceTier::Full).glyph_safe();
        assert!(!policy.decorative_animations);
        assert!(policy.keep_spinner);
        assert!(policy.simplified_render);
        let mut gated = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        gated.apply_glyph_safety(true);
        assert!(!gated.decorative_animations);
        let mut untouched = TuiPerfPolicy::for_tier(PerformanceTier::Full);
        untouched.apply_glyph_safety(false);
        assert!(untouched.decorative_animations);
    }
}
