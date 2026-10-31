//! Task completion and approval waiting notifications for CLI frontends.
//!
//! The engine runs on the server while the user may have backgrounded the
//! terminal, so long turns and pending tool approvals need an out-of-band
//! nudge. This module builds the notice text once and fans it out through
//! four channels in priority order: terminal-native escape sequences,
//! the BEL bell, an optional sound-pack cue, and finally an OS-native
//! banner spawned in the background.
//!
//! Both the mini session and the full TUI share this module; callers only
//! supply the event plus their best-known focus state. Terminal I/O never
//! blocks the caller: slow paths (process spawn) are detached, and nothing
//! is written when neither stdout nor stderr is a TTY.

use std::io::{IsTerminal, Write};
use std::time::Duration;

use crate::app_config::NotificationConfig;

/// Fallback turn length before a finish notice fires when unset.
const DEFAULT_MIN_DURATION_SECS: u64 = 8;

/// Notification id shared by every emitted sequence of one notice.
const NOTICE_ID: &str = "wf-task";

/// Best-known terminal focus state supplied by the caller. The mini session
/// has no focus tracking and always reports unknown; the full TUI derives
/// it from its focus events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusState {
    Focused,
    Blurred,
    Unknown,
}

/// How a turn ended, driving the notice title and status word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEnd {
    Completed,
    Failed,
    Interrupted,
}

impl TurnEnd {
    fn title(self) -> &'static str {
        match self {
            Self::Completed => "WF done",
            Self::Failed => "WF failed",
            Self::Interrupted => "WF interrupted",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::Completed => "Done",
            Self::Failed => "Failed",
            Self::Interrupted => "Interrupted",
        }
    }
}

/// Effective notification settings with every switch resolved to a concrete
/// value. Missing config entries fall back to on, matching the reference
/// behavior where notifications work out of the box.
#[derive(Debug, Clone)]
pub struct ResolvedNotifications {
    pub enabled: bool,
    pub turn_complete: bool,
    pub min_duration: Duration,
    pub terminal: bool,
    pub system: bool,
    pub bell: bool,
    pub background_only: bool,
    pub sound: bool,
    /// Playback volume normalized to 0.0-1.0.
    pub volume: f64,
    /// Configured sound-pack name or path, if any.
    pub sound_pack: Option<String>,
}

impl ResolvedNotifications {
    /// Resolve one parsed config section into effective settings.
    pub fn resolve(cfg: &NotificationConfig) -> Self {
        Self {
            enabled: cfg.enabled.unwrap_or(true),
            turn_complete: cfg.turn_complete.unwrap_or(true),
            min_duration: Duration::from_secs(
                cfg.turn_complete_min_secs
                    .unwrap_or(DEFAULT_MIN_DURATION_SECS),
            ),
            terminal: cfg.terminal.unwrap_or(true),
            system: cfg.system.unwrap_or(true),
            bell: cfg.bell.unwrap_or(true),
            background_only: cfg.background_only.unwrap_or(true),
            sound: cfg.sound.unwrap_or(true),
            volume: normalize_volume(cfg.volume.unwrap_or(1.0)),
            sound_pack: cfg
                .sound_pack
                .clone()
                .map(|pack| pack.trim().to_string())
                .filter(|pack| !pack.is_empty()),
        }
    }

    /// Load the live config file and resolve it. Any failure (missing or
    /// unreadable file) yields defaults, so notifications keep working on
    /// a fresh machine.
    pub fn load() -> Self {
        let cfg = crate::app_config::config_file_path()
            .and_then(|path| crate::app_config::ConfigManager::load(&path).ok())
            .map(|manager| manager.config().notifications.clone())
            .unwrap_or_default();
        Self::resolve(&cfg)
    }

    /// Nudge the user about a tool approval waiting for an answer. Short
    /// waits have no gate: a blocked approval always wants attention.
    pub fn notify_approval(&self, tool_name: &str, detail: Option<&str>, focus: FocusState) {
        if !self.enabled || self.suppressed_by_focus(focus) {
            return;
        }
        let body = match detail.filter(|text| !text.trim().is_empty()) {
            Some(detail) => format!("{tool_name} is waiting for approval ({})", detail.trim()),
            None => format!("{tool_name} is waiting for approval"),
        };
        self.dispatch("WF approval needed", &body, focus, SoundEvent::Approval);
    }

    /// Nudge the user that a turn ended. Short or disabled turns stay
    /// silent; the detail carries context such as the iteration count.
    pub fn notify_turn_finished(
        &self,
        elapsed: Duration,
        end: TurnEnd,
        detail: Option<&str>,
        focus: FocusState,
    ) {
        if !self.enabled || !self.turn_complete || elapsed < self.min_duration {
            return;
        }
        if self.suppressed_by_focus(focus) {
            return;
        }
        let mut body = format!("{} in {}", end.status(), fmt_duration(elapsed));
        if let Some(detail) = detail.filter(|text| !text.trim().is_empty()) {
            body.push_str(" · ");
            body.push_str(detail.trim());
        }
        self.dispatch(end.title(), &body, focus, SoundEvent::from(end));
    }

    /// Background-only mode drops the notice while the terminal is known
    /// to be focused; an unknown focus still notifies through the polite
    /// channels below.
    fn suppressed_by_focus(&self, focus: FocusState) -> bool {
        self.background_only && focus == FocusState::Focused
    }

    /// System banners are the most disruptive channel, so they only fire
    /// when the terminal is known to be blurred (or the background-only
    /// gate is off entirely).
    fn system_allowed(&self, focus: FocusState) -> bool {
        self.system && (!self.background_only || focus == FocusState::Blurred)
    }

    /// Sound cues are more disruptive than the bell, so they share the
    /// system-banner gate: only when the terminal is known to be blurred
    /// (or the background-only gate is off entirely). Unlike the banner,
    /// sound still plays when the terminal protocols delivered.
    fn sound_allowed(&self, focus: FocusState) -> bool {
        self.sound && self.volume > 0.0 && (!self.background_only || focus == FocusState::Blurred)
    }

    /// Emit through terminal escapes first, then the bell and an optional
    /// sound cue, then the OS banner only when the terminal protocols
    /// could not deliver.
    fn dispatch(&self, title: &str, body: &str, focus: FocusState, sound_event: SoundEvent) {
        let title = sanitize(title);
        let body = sanitize(body);
        let delivered = self.terminal && emit_terminal_notice(&title, &body);
        if self.bell {
            emit_bell();
        }
        if self.sound_allowed(focus) {
            play_sound(self.sound_pack.as_deref(), sound_event, self.volume);
        }
        if self.system_allowed(focus) && !delivered {
            spawn_system_notice(title, body);
        }
    }
}

/// Terminal families with a native notification protocol. Terminals absent
/// from this list fall back to the bell and the OS banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalApp {
    Kitty,
    WezTerm,
    Ghostty,
    ITerm2,
}

/// Pick the terminal-native protocol from well-known environment markers.
fn detect_terminal_app() -> Option<TerminalApp> {
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return Some(TerminalApp::Kitty);
    }
    if std::env::var_os("WEZTERM_PANE").is_some() {
        return Some(TerminalApp::WezTerm);
    }
    if std::env::var_os("WT_SESSION").is_some() {
        return None;
    }
    let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    if program.eq_ignore_ascii_case("wezterm") {
        return Some(TerminalApp::WezTerm);
    }
    if program.eq_ignore_ascii_case("ghostty") {
        return Some(TerminalApp::Ghostty);
    }
    if program == "iTerm.app" || program.eq_ignore_ascii_case("iTerm2") {
        return Some(TerminalApp::ITerm2);
    }
    if program.eq_ignore_ascii_case("apple_terminal")
        || program.eq_ignore_ascii_case("terminal.app")
        || program.eq_ignore_ascii_case("terminal")
        || program.eq_ignore_ascii_case("windows_terminal")
    {
        return None;
    }
    let lc_terminal = std::env::var("LC_TERMINAL").unwrap_or_default();
    if lc_terminal.eq_ignore_ascii_case("iTerm2") {
        return Some(TerminalApp::ITerm2);
    }
    if std::env::var("TERM").unwrap_or_default().contains("kitty") {
        return Some(TerminalApp::Kitty);
    }
    None
}

/// Write one notice through the terminal-native protocol. Returns whether
/// a supporting terminal accepted the sequences.
fn emit_terminal_notice(title: &str, body: &str) -> bool {
    let Some(app) = detect_terminal_app() else {
        return false;
    };
    let sequence = terminal_sequence(app, title, body);
    if std::io::stdout().is_terminal() {
        let mut out = std::io::stdout();
        if out.write_all(sequence.as_bytes()).is_ok() && out.flush().is_ok() {
            return true;
        }
    }
    if std::io::stderr().is_terminal() {
        let mut err = std::io::stderr();
        if err.write_all(sequence.as_bytes()).is_ok() && err.flush().is_ok() {
            return true;
        }
    }
    false
}

/// Build the raw escape sequence for one terminal family and notice.
fn terminal_sequence(app: TerminalApp, title: &str, body: &str) -> String {
    match app {
        TerminalApp::Kitty => format!(
            "\x1b]99;i={NOTICE_ID}:o=unfocused:d=0;{title}\x1b\\\
             \x1b]99;i={NOTICE_ID}:p=body;{body}\x1b\\"
        ),
        TerminalApp::WezTerm | TerminalApp::Ghostty => {
            let title = title.replace(';', ":");
            let body = body.replace(';', ":");
            format!("\x1b]777;notify;{title};{body}\x1b\\")
        }
        TerminalApp::ITerm2 => {
            let payload = match (title.trim().is_empty(), body.trim().is_empty()) {
                (false, false) => format!("{title}: {body}"),
                (false, true) => title.to_string(),
                (true, false) => body.to_string(),
                (true, true) => "WF".to_string(),
            };
            format!("\x1b]9;{payload}\x1b\\")
        }
    }
}

/// Ring the terminal bell on the first TTY stream. Terminals decide whether
/// that means a sound, a taskbar flash, or nothing.
fn emit_bell() {
    const BELL: &[u8] = b"\x07";
    if std::io::stdout().is_terminal() {
        let mut out = std::io::stdout();
        let _ = out.write_all(BELL).and(out.flush());
        return;
    }
    if std::io::stderr().is_terminal() {
        let mut err = std::io::stderr();
        let _ = err.write_all(BELL).and(err.flush());
    }
}

/// Notice event selecting the sound-pack file. Approval and turn-end
/// cues stay distinct so the user can tell them apart by ear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundEvent {
    Approval,
    Done,
    Failed,
    Interrupted,
}

impl SoundEvent {
    fn file_stem(self) -> &'static str {
        match self {
            Self::Approval => "approval",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

impl From<TurnEnd> for SoundEvent {
    fn from(end: TurnEnd) -> Self {
        match end {
            TurnEnd::Completed => Self::Done,
            TurnEnd::Failed => Self::Failed,
            TurnEnd::Interrupted => Self::Interrupted,
        }
    }
}

/// Clamp a configured volume to 0.0-1.0. Non-finite values fall back to
/// full volume; 0.0 mutes playback and skips the player entirely.
fn normalize_volume(volume: f64) -> f64 {
    if !volume.is_finite() {
        return 1.0;
    }
    volume.clamp(0.0, 1.0)
}

/// Audio file extensions tried per event, in preference order.
const SOUND_EXTENSIONS: [&str; 4] = ["wav", "aiff", "mp3", "ogg"];

/// Resolve a configured pack to its directory. Absolute paths are used
/// directly; relative paths resolve against the config file directory
/// first, then its sounds subdirectory for bare pack names.
fn resolve_pack_dir(pack: &str) -> Option<std::path::PathBuf> {
    let config_dir = crate::app_config::config_file_path()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))?;
    resolve_pack_dir_in(&config_dir, pack)
}

/// Pack resolution against an explicit base directory, so tests can use
/// a scratch dir instead of the live config location.
fn resolve_pack_dir_in(base: &std::path::Path, pack: &str) -> Option<std::path::PathBuf> {
    let trimmed = pack.trim();
    if trimmed.is_empty() {
        return None;
    }
    let direct = std::path::Path::new(trimmed);
    if direct.is_absolute() {
        return direct.is_dir().then(|| direct.to_path_buf());
    }
    let relative = base.join(trimmed);
    if relative.is_dir() {
        return Some(relative);
    }
    let named = base.join("sounds").join(trimmed);
    named.is_dir().then_some(named)
}

/// First playable file for one event inside a pack directory.
fn find_event_file(dir: &std::path::Path, event: SoundEvent) -> Option<std::path::PathBuf> {
    let stem = event.file_stem();
    SOUND_EXTENSIONS
        .iter()
        .map(|ext| dir.join(format!("{stem}.{ext}")))
        .find(|path| path.is_file())
}

/// External audio players per platform, in preference order. Windows has
/// none: shelling out to a helper has been observed to break fullscreen
/// TUI sessions, so sound stays silent there until proven safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundPlayer {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Afplay,
    Paplay,
    Ffplay,
    Aplay,
}

/// Look up one executable on PATH without spawning a shell.
#[cfg(not(target_os = "windows"))]
fn find_on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// First available player for this OS. The PATH scan runs once per
/// process; playback itself stays on detached threads.
fn cached_sound_player() -> Option<SoundPlayer> {
    static PLAYER: std::sync::OnceLock<Option<SoundPlayer>> = std::sync::OnceLock::new();
    *PLAYER.get_or_init(detect_sound_player)
}

fn detect_sound_player() -> Option<SoundPlayer> {
    #[cfg(target_os = "macos")]
    {
        if find_on_path("afplay").is_some() {
            return Some(SoundPlayer::Afplay);
        }
        if find_on_path("ffplay").is_some() {
            return Some(SoundPlayer::Ffplay);
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        if find_on_path("paplay").is_some() {
            return Some(SoundPlayer::Paplay);
        }
        if find_on_path("ffplay").is_some() {
            return Some(SoundPlayer::Ffplay);
        }
        if find_on_path("aplay").is_some() {
            return Some(SoundPlayer::Aplay);
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return None;
    }
}

/// Build the player invocation for one file. Volume maps onto each
/// player's native scale; players without volume control play at the
/// system volume.
fn player_command(
    player: SoundPlayer,
    file: &std::path::Path,
    volume: f64,
) -> (String, Vec<String>) {
    let file = file.to_string_lossy().into_owned();
    match player {
        SoundPlayer::Afplay => (
            "afplay".to_string(),
            vec!["-v".to_string(), format!("{volume:.2}"), file],
        ),
        SoundPlayer::Paplay => {
            let level = (volume * 65536.0).round().clamp(0.0, 65536.0) as u32;
            (
                "paplay".to_string(),
                vec![format!("--volume={level}"), file],
            )
        }
        SoundPlayer::Ffplay => {
            let level = (volume * 100.0).round().clamp(0.0, 100.0) as u32;
            (
                "ffplay".to_string(),
                vec![
                    "-nodisp".to_string(),
                    "-autoexit".to_string(),
                    "-loglevel".to_string(),
                    "quiet".to_string(),
                    "-volume".to_string(),
                    level.to_string(),
                    file,
                ],
            )
        }
        SoundPlayer::Aplay => ("aplay".to_string(), vec![file]),
    }
}

/// Play one pack cue in a detached thread. Every miss (no pack, no file,
/// no player, failed spawn) is silent: sound is best-effort decoration
/// that must never disturb the turn pipeline.
fn play_sound(pack: Option<&str>, event: SoundEvent, volume: f64) {
    if cfg!(target_os = "windows") {
        return;
    }
    if volume <= 0.0 {
        return;
    }
    let Some(pack) = pack else {
        return;
    };
    let Some(dir) = resolve_pack_dir(pack) else {
        return;
    };
    let Some(file) = find_event_file(&dir, event) else {
        return;
    };
    std::thread::spawn(move || {
        let Some(player) = cached_sound_player() else {
            return;
        };
        let (program, args) = player_command(player, &file, volume);
        let _ = std::process::Command::new(program).args(&args).output();
    });
}

/// Hand the banner to the OS in a detached thread so the caller never
/// blocks on a helper process. Windows stays silent: shelling out to a
/// notification helper has been observed to break fullscreen TUI sessions.
fn spawn_system_notice(title: String, body: String) {
    if cfg!(target_os = "windows") {
        return;
    }
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        {
            let script = format!(
                "display notification {} with title {}",
                apple_script_string(&body),
                apple_script_string(&title),
            );
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .output();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = std::process::Command::new("notify-send")
                .arg(&title)
                .arg(&body)
                .output();
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = (title, body);
        }
    });
}

/// Quote one string as an AppleScript string literal.
#[cfg(target_os = "macos")]
fn apple_script_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Compact duration rendering for notice bodies.
fn fmt_duration(duration: Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

/// Strip control characters and collapse whitespace so notice text can
/// never terminate an OSC sequence early.
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> NotificationConfig {
        NotificationConfig {
            enabled: Some(true),
            turn_complete: Some(true),
            turn_complete_min_secs: Some(5),
            turn_complete_todo_min_secs: None,
            terminal: Some(true),
            system: Some(true),
            bell: Some(true),
            background_only: Some(true),
            sound: None,
            volume: None,
            sound_pack: None,
        }
    }

    #[test]
    fn defaults_resolve_to_on_with_fallback_duration() {
        let resolved = ResolvedNotifications::resolve(&NotificationConfig::default());
        assert!(resolved.enabled);
        assert!(resolved.turn_complete);
        assert!(resolved.terminal);
        assert!(resolved.system);
        assert!(resolved.bell);
        assert!(resolved.background_only);
        assert_eq!(
            resolved.min_duration,
            Duration::from_secs(DEFAULT_MIN_DURATION_SECS)
        );
    }

    #[test]
    fn explicit_values_survive_resolution() {
        let cfg = NotificationConfig {
            enabled: Some(false),
            turn_complete_min_secs: Some(42),
            ..NotificationConfig::default()
        };
        let resolved = ResolvedNotifications::resolve(&cfg);
        assert!(!resolved.enabled);
        assert_eq!(resolved.min_duration, Duration::from_secs(42));
    }

    #[test]
    fn focused_state_suppresses_background_only() {
        let resolved = ResolvedNotifications::resolve(&enabled_config());
        assert!(resolved.suppressed_by_focus(FocusState::Focused));
        assert!(!resolved.suppressed_by_focus(FocusState::Blurred));
        assert!(!resolved.suppressed_by_focus(FocusState::Unknown));
    }

    #[test]
    fn system_channel_needs_blur_under_background_only() {
        let resolved = ResolvedNotifications::resolve(&enabled_config());
        assert!(resolved.system_allowed(FocusState::Blurred));
        assert!(!resolved.system_allowed(FocusState::Unknown));
        assert!(!resolved.system_allowed(FocusState::Focused));
    }

    #[test]
    fn durations_render_compactly() {
        assert_eq!(fmt_duration(Duration::from_millis(300)), "300ms");
        assert_eq!(fmt_duration(Duration::from_secs(12)), "12.0s");
    }

    #[test]
    fn sanitize_removes_sequence_breaks() {
        assert_eq!(sanitize("a\x07b\x1bc d"), "a b c d");
        assert_eq!(sanitize("  padded\ttext  "), "padded text");
    }

    #[test]
    fn kitty_sequence_carries_notice_id() {
        let sequence = terminal_sequence(TerminalApp::Kitty, "WF done", "Done in 3.0s");
        assert!(sequence.contains(NOTICE_ID));
        assert!(sequence.contains("WF done"));
    }

    #[test]
    fn osc777_sequence_avoids_semicolons() {
        let sequence = terminal_sequence(TerminalApp::WezTerm, "a;b", "c;d");
        assert_eq!(sequence, "\x1b]777;notify;a:b;c:d\x1b\\");
    }

    #[test]
    fn iterm2_sequence_falls_back_to_sender() {
        let sequence = terminal_sequence(TerminalApp::ITerm2, "", "");
        assert!(sequence.contains("WF"));
    }

    #[test]
    fn sound_defaults_to_on_full_volume_without_pack() {
        let resolved = ResolvedNotifications::resolve(&NotificationConfig::default());
        assert!(resolved.sound);
        assert_eq!(resolved.volume, 1.0);
        assert_eq!(resolved.sound_pack, None);
    }

    #[test]
    fn sound_pack_blank_resolves_to_none() {
        let cfg = NotificationConfig {
            sound_pack: Some("   ".to_string()),
            ..NotificationConfig::default()
        };
        let resolved = ResolvedNotifications::resolve(&cfg);
        assert_eq!(resolved.sound_pack, None);
    }

    #[test]
    fn volumes_normalize_to_unit_range() {
        assert_eq!(normalize_volume(0.5), 0.5);
        assert_eq!(normalize_volume(0.0), 0.0);
        assert_eq!(normalize_volume(1.0), 1.0);
        assert_eq!(normalize_volume(-0.5), 0.0);
        assert_eq!(normalize_volume(2.0), 1.0);
        assert_eq!(normalize_volume(f64::NAN), 1.0);
        assert_eq!(normalize_volume(f64::INFINITY), 1.0);
    }

    #[test]
    fn sound_shares_blur_gate_with_system_banner() {
        let resolved = ResolvedNotifications::resolve(&enabled_config());
        assert!(resolved.sound_allowed(FocusState::Blurred));
        assert!(!resolved.sound_allowed(FocusState::Unknown));
        assert!(!resolved.sound_allowed(FocusState::Focused));
    }

    #[test]
    fn sound_events_map_to_distinct_stems() {
        assert_eq!(SoundEvent::Approval.file_stem(), "approval");
        assert_eq!(SoundEvent::from(TurnEnd::Completed), SoundEvent::Done);
        assert_eq!(SoundEvent::from(TurnEnd::Failed), SoundEvent::Failed);
        assert_eq!(
            SoundEvent::from(TurnEnd::Interrupted),
            SoundEvent::Interrupted
        );
    }

    #[test]
    fn player_commands_carry_scaled_volumes() {
        let file = std::path::Path::new("/tmp/cue.wav");
        let (program, args) = player_command(SoundPlayer::Paplay, file, 0.5);
        assert_eq!(program, "paplay");
        assert_eq!(
            args,
            vec!["--volume=32768".to_string(), "/tmp/cue.wav".to_string()]
        );
        let (program, args) = player_command(SoundPlayer::Ffplay, file, 0.5);
        assert_eq!(program, "ffplay");
        assert!(args.contains(&"-volume".to_string()));
        assert!(args.contains(&"50".to_string()));
        assert!(args.contains(&"-nodisp".to_string()));
        let (program, args) = player_command(SoundPlayer::Afplay, file, 0.5);
        assert_eq!(program, "afplay");
        assert!(args.contains(&"-v".to_string()));
        let (program, args) = player_command(SoundPlayer::Aplay, file, 0.5);
        assert_eq!(program, "aplay");
        assert_eq!(args, vec!["/tmp/cue.wav".to_string()]);
    }

    /// Scratch pack directory under the system temp dir, removed on drop.
    struct ScratchPack {
        dir: std::path::PathBuf,
    }

    impl ScratchPack {
        fn create(files: &[&str]) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "wf-notify-pack-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock before epoch")
                    .as_nanos(),
            ));
            std::fs::create_dir_all(&dir).expect("scratch pack dir");
            for file in files {
                std::fs::write(dir.join(file), b"RIFF").expect("scratch cue file");
            }
            Self { dir }
        }
    }

    impl Drop for ScratchPack {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn pack_resolution_prefers_directories() {
        let pack = ScratchPack::create(&["done.wav"]);
        let base = std::env::temp_dir();
        assert_eq!(resolve_pack_dir_in(&base, "  "), None);
        assert_eq!(resolve_pack_dir_in(&base, "/definitely/absent/pack"), None);
        let absolute = pack.dir.to_string_lossy().into_owned();
        assert_eq!(
            resolve_pack_dir_in(&base, &absolute),
            Some(pack.dir.clone())
        );
    }

    #[test]
    fn event_files_prefer_wav_over_later_formats() {
        let pack = ScratchPack::create(&["done.mp3", "done.wav", "approval.ogg"]);
        assert_eq!(
            find_event_file(&pack.dir, SoundEvent::Done),
            Some(pack.dir.join("done.wav"))
        );
        assert_eq!(
            find_event_file(&pack.dir, SoundEvent::Approval),
            Some(pack.dir.join("approval.ogg"))
        );
        assert_eq!(find_event_file(&pack.dir, SoundEvent::Failed), None);
    }
}
