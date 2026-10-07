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

pub mod cache;
pub mod data;
pub mod derive;
pub mod literal;
pub mod osc;
pub mod resolve;
pub mod style;

pub use cache::{
    load_theme_cache, load_theme_file, load_theme_overrides, save_theme_cache, theme_cache_path,
    theme_config_path, theme_overrides_path,
};
pub use data::{
    to_bold_style, to_dim_style, to_ratatui_color, to_style, ColorDomain, ColorRole, Rgb, Theme,
    ThemeKind, ThemeOverrides, ThemeSource,
};
pub use derive::{blend_rgb, derive_theme, is_light, luminance};
pub use literal::{nearest_role, remap_literal};
pub use osc::{parse_osc_response, OscColorParser};
pub use resolve::{
    fallback_theme, probe_theme, probe_theme_with_timeout, resolve_theme, OSC_PROBE_TIMEOUT,
};
pub use style::{
    accent_style, assistant_message_style, error_style, highlight_style, muted_style,
    success_style, table_separator_style, tool_call_style, user_message_style, warning_style,
};

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

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
