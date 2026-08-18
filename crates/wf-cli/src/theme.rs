//! Theme detection: OSC 10/11 color probing, palette derivation, the
//! last-known-good cache and the SIGUSR2 hot-reload signal 
//!
//! The theme is pure data ([`Theme`]) consumed by later stages (Stage 4+
//! components map roles to ratatui styles); this module never renders.
//!
//! Probe pipeline (unix): open `/dev/tty` → temporarily enable raw mode →
//! write the OSC 11 (background) and OSC 10 (foreground) queries → read
//! responses with [`OSC_PROBE_TIMEOUT`] → restore the raw-mode state →
//! derive the theme. Every failure mode (no `/dev/tty`, no response before
//! the timeout, garbage bytes) degrades gracefully: last-known-good cache →
//! built-in dark theme. [`probe_theme`] never panics and reports which
//! source produced the theme.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Default OSC response wait
pub const OSC_PROBE_TIMEOUT: Duration = Duration::from_millis(100);

// ── pure data ─────────────────────────────────────────────────────────

/// 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Hex form `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Linear-ish blend: `t = 0` → `self`, `t = 1` → `other`.
    fn blend(self, other: Rgb, t: f32) -> Rgb {
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
        Rgb::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
        )
    }
}

/// Dark or light theme, derived from the background luminance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Dark,
    Light,
}

/// How the returned theme came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeSource {
    /// Live OSC 10/11 response.
    Probed,
    /// Last-known-good cache file.
    Cached,
    /// Built-in fallback.
    #[default]
    Default,
}

/// Color-domain capability for later ANSI mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDomain {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl ColorDomain {
    /// Detect from `COLORTERM` / `TERM` (pure; env injected for tests).
    pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Self {
        match colorterm.map(str::to_ascii_lowercase).as_deref() {
            Some("truecolor") | Some("24bit") => return Self::TrueColor,
            _ => {}
        }
        if term.map(|t| t.contains("256color")).unwrap_or(false) {
            Self::Ansi256
        } else {
            Self::Ansi16
        }
    }

    /// Detect from the real environment.
    pub fn detect_from_env() -> Self {
        Self::detect(std::env::var("COLORTERM").ok().as_deref(), std::env::var("TERM").ok().as_deref())
    }
}

/// Terminal theme: the 8 ColorRoles as pure RGB data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub kind: ThemeKind,
    /// Default role (text).
    pub fg: Rgb,
    /// Canvas.
    pub bg: Rgb,
    /// Muted role (dimmed text).
    pub muted: Rgb,
    /// Brand / accent role.
    pub accent: Rgb,
    /// Additions (diff +, success).
    pub add: Rgb,
    /// Removals (diff -).
    pub remove: Rgb,
    /// Warnings.
    pub warning: Rgb,
    /// Errors.
    pub error: Rgb,
    /// Highlights / selection.
    pub highlight: Rgb,
    #[serde(skip)]
    pub source: ThemeSource,
}

impl Theme {
    /// Built-in dark fallback.
    pub fn dark_default() -> Self {
        Self {
            kind: ThemeKind::Dark,
            fg: Rgb::new(0xE5, 0xE7, 0xEB),
            bg: Rgb::new(0x0F, 0x14, 0x1A),
            muted: Rgb::new(0x8B, 0x93, 0x9E),
            accent: Rgb::new(0x22, 0xD3, 0xEE),
            add: Rgb::new(0x4A, 0xDE, 0x80),
            remove: Rgb::new(0xF8, 0x71, 0x71),
            warning: Rgb::new(0xFA, 0xCC, 0x15),
            error: Rgb::new(0xF8, 0x71, 0x71),
            highlight: Rgb::new(0x60, 0xA5, 0xFA),
            source: ThemeSource::Default,
        }
    }

    /// Built-in light fallback.
    pub fn light_default() -> Self {
        Self {
            kind: ThemeKind::Light,
            fg: Rgb::new(0x1F, 0x29, 0x37),
            bg: Rgb::new(0xFA, 0xFB, 0xFC),
            muted: Rgb::new(0x6B, 0x72, 0x80),
            accent: Rgb::new(0x0E, 0x74, 0x8C),
            add: Rgb::new(0x15, 0x80, 0x3D),
            remove: Rgb::new(0xB4, 0x23, 0x23),
            warning: Rgb::new(0xA1, 0x62, 0x07),
            error: Rgb::new(0xB4, 0x23, 0x23),
            highlight: Rgb::new(0x1D, 0x4E, 0xD8),
            source: ThemeSource::Default,
        }
    }
}

// ── derivation (pure) ─────────────────────────────────────────────────

/// Relative luminance in `0.0..=1.0` (ITU-R BT.601 weights).
pub fn luminance(c: Rgb) -> f32 {
    (0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32) / 255.0
}

/// Accent candidates; the winner maximizes contrast against the background.
const ACCENT_CANDIDATES: [Rgb; 4] = [
    Rgb::new(0x22, 0xD3, 0xEE), // cyan
    Rgb::new(0xA7, 0x8B, 0xFA), // violet
    Rgb::new(0xF5, 0x9E, 0x0B), // amber
    Rgb::new(0x2D, 0xD4, 0xBF), // teal
];

/// Luminance threshold below which a background is "dark".
const DARK_LUMINANCE_THRESHOLD: f32 = 0.5;

/// Derive a full theme from a background color (foreground defaults to the
/// classic white/black contrasting default when `fg` is `None`).
pub fn derive_theme(bg: Rgb, fg: Option<Rgb>) -> Theme {
    let kind = if luminance(bg) < DARK_LUMINANCE_THRESHOLD {
        ThemeKind::Dark
    } else {
        ThemeKind::Light
    };
    let default_fg = match kind {
        ThemeKind::Dark => Rgb::new(0xE5, 0xE7, 0xEB),
        ThemeKind::Light => Rgb::new(0x1F, 0x29, 0x37),
    };
    let fg = fg.unwrap_or(default_fg);

    let bg_lum = luminance(bg);
    let accent = ACCENT_CANDIDATES
        .into_iter()
        .max_by(|a, b| {
            let da = (luminance(*a) - bg_lum).abs();
            let db = (luminance(*b) - bg_lum).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(Rgb::new(0x22, 0xD3, 0xEE));

    // Role variants follow the theme kind (dark → brighter, light → deeper).
    let (add, remove, warning, error, highlight) = match kind {
        ThemeKind::Dark => (
            Rgb::new(0x4A, 0xDE, 0x80),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0xFA, 0xCC, 0x15),
            Rgb::new(0xF8, 0x71, 0x71),
            Rgb::new(0x60, 0xA5, 0xFA),
        ),
        ThemeKind::Light => (
            Rgb::new(0x15, 0x80, 0x3D),
            Rgb::new(0xB4, 0x23, 0x23),
            Rgb::new(0xA1, 0x62, 0x07),
            Rgb::new(0xB4, 0x23, 0x23),
            Rgb::new(0x1D, 0x4E, 0xD8),
        ),
    };

    Theme {
        kind,
        fg,
        bg,
        muted: fg.blend(bg, 0.55),
        accent,
        add,
        remove,
        warning,
        error,
        highlight,
        source: ThemeSource::Probed,
    }
}

// ── OSC response parsing (pure) ───────────────────────────────────────

/// Incremental parser for OSC 10/11 color responses
/// (`ESC ] 11 ; rgb:RRRR/GGGG/BBBB ST`, ST = `ESC \` or BEL). Non-OSC bytes
/// are dropped, so responses split across arbitrary chunk boundaries still
/// parse.
#[derive(Debug, Default)]
pub struct OscColorParser {
    buf: Vec<u8>,
    scan_from: usize,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
}

impl OscColorParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the next chunk of terminal bytes.
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
        self.scan();
    }

    /// Both responses captured.
    pub fn is_complete(&self) -> bool {
        self.fg.is_some() && self.bg.is_some()
    }

    /// Extract the captured colors.
    pub fn finish(self) -> (Option<Rgb>, Option<Rgb>) {
        (self.fg, self.bg)
    }

    fn scan(&mut self) {
        loop {
            // Skip garbage up to the next ESC.
            let start = self.buf[self.scan_from..]
                .iter()
                .position(|&b| b == 0x1b)
                .map(|i| self.scan_from + i);
            let Some(start) = start else {
                self.scan_from = self.buf.len();
                self.drain_scanned();
                return;
            };
            self.buf.drain(..start);
            self.scan_from = 0;

            match parse_osc_response(&self.buf) {
                Some((query, rgb, consumed)) => {
                    match query {
                        10 => self.fg = Some(rgb),
                        11 => self.bg = Some(rgb),
                        _ => {}
                    }
                    self.buf.drain(..consumed);
                    if self.is_complete() {
                        return;
                    }
                }
                // Incomplete response at the buffer end → wait for more.
                None if looks_like_incomplete_response(&self.buf) => {
                    self.drain_scanned();
                    return;
                }
                // ESC that is not an OSC color response → skip this byte.
                None => {
                    self.buf.drain(..1);
                }
            }
        }
    }

    /// Drop already-scanned garbage so the buffer stays bounded.
    fn drain_scanned(&mut self) {
        if self.scan_from > 0 && self.scan_from == self.buf.len() {
            self.buf.clear();
            self.scan_from = 0;
        }
    }
}

/// Whether the buffer starts with a potentially-incomplete OSC color
/// response (a prefix of `\x1b]1x;rgb:…`).
fn looks_like_incomplete_response(buf: &[u8]) -> bool {
    let Some(rest) = buf.strip_prefix(b"\x1b]") else {
        return false;
    };
    for header in [&b"10;rgb:"[..], &b"11;rgb:"[..]] {
        if rest.len() < header.len() && header.starts_with(rest) {
            return true; // still receiving the header
        }
        if rest.starts_with(header) {
            return true; // payload / terminator not fully arrived
        }
    }
    false
}

/// Parse one complete OSC color response at the start of `buf`.
/// Returns `(query, rgb, bytes_consumed)`.
pub fn parse_osc_response(buf: &[u8]) -> Option<(u8, Rgb, usize)> {
    let rest = buf.strip_prefix(b"\x1b]")?;
    let (query, rest) = if let Some(r) = rest.strip_prefix(b"10;") {
        (10u8, r)
    } else if let Some(r) = rest.strip_prefix(b"11;") {
        (11u8, r)
    } else {
        return None;
    };
    let rest = rest.strip_prefix(b"rgb:")?;

    let mut pos = 0usize;
    let mut values = [0u8; 3];
    for (idx, value) in values.iter_mut().enumerate() {
        if idx > 0 {
            if rest.get(pos) == Some(&b'/') {
                pos += 1;
            } else {
                return None;
            }
        }
        let start = pos;
        while pos < rest.len() && (rest[pos] as char).is_ascii_hexdigit() && pos - start < 4 {
            pos += 1;
        }
        let len = pos - start;
        if len == 0 {
            return None;
        }
        let digits = std::str::from_utf8(&rest[start..pos]).ok()?;
        let raw = u16::from_str_radix(digits, 16).ok()?;
        *value = match len {
            1 => (raw as u8) * 17,
            2 => raw as u8,
            3 => ((raw as u32 * 255) / 0xFFF) as u8,
            _ => (raw >> 8) as u8,
        };
    }

    // Terminator: BEL or `ESC \`.
    let terminator_len = match rest.get(pos) {
        Some(0x07) => 1,
        Some(0x1b) if rest.get(pos + 1) == Some(&b'\\') => 2,
        _ => return None,
    };
    // `\x1b]` + `1x;` + `rgb:` + payload + terminator
    let consumed = 2 + 3 + 4 + pos + terminator_len;
    Some((query, Rgb::new(values[0], values[1], values[2]), consumed))
}

// ── last-known-good cache ─────────────────────────────────────────────

/// Cache file path: `$XDG_CACHE_HOME/wf-cli/theme.json` (fallback
/// `$HOME/.cache/wf-cli/theme.json`). `None` when no home is discoverable.
pub fn theme_cache_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("wf-cli").join("theme.json"))
}

/// Persist a successfully probed theme (best effort; the `source` field is
/// serde-skipped and always loads back as [`ThemeSource::Cached`]).
pub fn save_theme_cache(theme: &Theme) {
    let Some(path) = theme_cache_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let _ = std::fs::write(path, serde_json::to_vec_pretty(theme).unwrap_or_default());
}

/// Load the last-known-good theme, marked [`ThemeSource::Cached`].
pub fn load_theme_cache() -> Option<Theme> {
    let path = theme_cache_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let mut theme: Theme = serde_json::from_str(&raw).ok()?;
    theme.source = ThemeSource::Cached;
    Some(theme)
}

// ── live probing ──────────────────────────────────────────────────────

/// Probe the terminal theme (OSC 11 background + OSC 10 foreground) with
/// the default timeout; never panics — failures fall back to the cache /
/// built-in dark theme.
pub fn probe_theme() -> Theme {
    probe_theme_with_timeout(OSC_PROBE_TIMEOUT)
}

/// [`probe_theme`] with an explicit timeout.
pub fn probe_theme_with_timeout(timeout: Duration) -> Theme {
    match probe_osc_colors(timeout) {
        (Some(bg), fg) => {
            let theme = derive_theme(bg, fg);
            save_theme_cache(&theme);
            theme
        }
        (None, _) => fallback_theme(),
    }
}

/// Fallback chain: last-known-good cache → built-in dark theme.
pub fn fallback_theme() -> Theme {
    load_theme_cache().unwrap_or_else(Theme::dark_default)
}

#[cfg(unix)]
fn probe_osc_colors(timeout: Duration) -> (Option<Rgb>, Option<Rgb>) {
    use std::os::unix::io::AsRawFd;

    // A controlling terminal is required; containers/CI often lack one.
    let Ok(mut tty) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    else {
        return (None, None);
    };

    let raw_was_enabled = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    let raw_restorer = RawModeRestorer { restore: raw_was_enabled };
    if !raw_was_enabled && crossterm::terminal::enable_raw_mode().is_err() {
        return (None, None);
    }

    // Both queries in one write; responses may arrive in any order.
    const QUERIES: &[u8] = b"\x1b]11;?\x1b\\\x1b]10;?\x1b\\";
    if tty.write_all(QUERIES).is_err() || tty.flush().is_err() {
        drop(raw_restorer);
        return (None, None);
    }

    let deadline = Instant::now() + timeout;
    let mut parser = OscColorParser::new();
    let mut chunk = [0u8; 256];
    let fd = tty.as_raw_fd();
    while !parser.is_complete() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `pfd` is a valid, exclusively-owned pollfd.
        let ready = unsafe {
            libc::poll(&mut pfd, 1, remaining.as_millis().clamp(1, i32::MAX as u128) as i32)
        };
        if ready <= 0 {
            break; // timeout or error
        }
        match tty.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => parser.feed(&chunk[..n]),
            Err(_) => break,
        }
    }
    drop(raw_restorer);
    parser.finish()
}

#[cfg(not(unix))]
fn probe_osc_colors(_timeout: Duration) -> (Option<Rgb>, Option<Rgb>) {
    (None, None)
}

/// Restores the raw-mode state found before probing (RAII).
struct RawModeRestorer {
    restore: bool,
}

impl Drop for RawModeRestorer {
    fn drop(&mut self) {
        // When raw mode was already on the caller owns it; otherwise the
        // probe enabled it and must turn it back off.
        if !self.restore {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

// ── SIGUSR2 hot reload ────────────────────────────────────────────────

/// Channel delivering one `()` per SIGUSR2 (theme hot-reload requests,
/// 05 §3.3). Non-unix platforms get an immediately-closed channel.
#[cfg(unix)]
pub async fn theme_reload_signals() -> std::io::Result<tokio::sync::mpsc::Receiver<()>> {
    use tokio::signal::unix::{signal, SignalKind};

    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let mut stream = signal(SignalKind::user_defined2())?;
    tokio::spawn(async move {
        while stream.recv().await.is_some() {
            if tx.send(()).await.is_err() {
                break; // receiver dropped
            }
        }
    });
    Ok(rx)
}

#[cfg(not(unix))]
pub async fn theme_reload_signals() -> std::io::Result<tokio::sync::mpsc::Receiver<()>> {
    let (_tx, rx) = tokio::sync::mpsc::channel::<()>(8);
    Ok(rx) // _tx dropped → closed channel
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(ColorDomain::detect(Some("truecolor"), Some("xterm")), ColorDomain::TrueColor);
        assert_eq!(ColorDomain::detect(Some("24bit"), None), ColorDomain::TrueColor);
        assert_eq!(ColorDomain::detect(None, Some("xterm-256color")), ColorDomain::Ansi256);
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

    // ── probe degradation ─────────────────────────────────────────────

    #[test]
    fn probe_theme_never_panics_and_reports_a_source() {
        let _lock = ENV_LOCK.lock().unwrap();
        // In CI there is no controlling terminal / no OSC responder; on a
        // developer terminal this may legitimately probe. Either way a
        // fully-formed theme must come back.
        let theme = probe_theme_with_timeout(Duration::from_millis(20));
        let default = Theme::dark_default();
        assert_eq!(theme.kind, ThemeKind::Dark, "CI default is dark");
        assert_eq!(theme.fg, default.fg);
        assert_eq!(theme.bg, default.bg);
    }

    // ── SIGUSR2 ───────────────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn sigusr2_delivers_a_reload_signal() {
        let mut rx = theme_reload_signals().await.unwrap();
        // The handler is registered now; sending the signal is safe.
        unsafe {
            libc::kill(libc::getpid(), libc::SIGUSR2);
        }
        let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("signal within 2s");
        assert_eq!(received, Some(()));
    }
}
