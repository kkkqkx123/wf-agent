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
#[cfg(target_os = "linux")]
fn linux_player_commands_carry_scaled_volumes() {
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
    let (program, args) = player_command(SoundPlayer::Aplay, file, 0.5);
    assert_eq!(program, "aplay");
    assert_eq!(args, vec!["/tmp/cue.wav".to_string()]);
}

#[test]
#[cfg(target_os = "macos")]
fn macos_player_commands_carry_scaled_volumes() {
    let file = std::path::Path::new("/tmp/cue.wav");
    let (program, args) = player_command(SoundPlayer::Afplay, file, 0.5);
    assert_eq!(program, "afplay");
    assert!(args.contains(&"-v".to_string()));
    let (program, args) = player_command(SoundPlayer::Ffplay, file, 0.5);
    assert_eq!(program, "ffplay");
    assert!(args.contains(&"50".to_string()));
}

#[test]
#[cfg(target_os = "windows")]
fn windows_player_plays_wav_without_a_window() {
    let file = std::path::Path::new("C:\\sounds\\done.wav");
    let (program, args) = player_command(SoundPlayer::PowerShell, file, 0.5);
    assert_eq!(program, "powershell.exe");
    assert!(args.contains(&"-Command".to_string()));
    assert!(args.contains(&"Hidden".to_string()));
    let script = args.last().expect("player script");
    assert!(script.contains("System.Media.SoundPlayer"));
    assert!(script.contains("C:\\sounds\\done.wav"));
}

#[test]
#[cfg(target_os = "windows")]
fn powershell_quote_doubles_single_quotes() {
    assert_eq!(powershell_quote("it's"), "it''s");
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
