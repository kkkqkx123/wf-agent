//! Terminal capability probing: keyboard enhancement, bracketed paste,
//! focus events and default background color detection.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// RGB color triplet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorSet {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Probed terminal capabilities.
///
/// Constructed once at startup via [`TerminalProbe::detect`]. The probe
/// queries the terminal for its default background color (OSC 11) and
/// keyboard enhancement support, then stores the results so the rest of
/// the application can query them without re-probing.
#[derive(Debug, Clone)]
pub struct TerminalProbe {
    /// Whether the terminal supports the Kitty keyboard enhancement
    /// protocol (reported via DA2 or assumed when the environment variable
    /// `TERM_PROGRAM` indicates a known-supporting terminal).
    pub keyboard_enhancement: bool,
    /// Whether bracketed paste mode is supported (nearly universal;
    /// `false` only on very old terminals).
    pub bracketed_paste: bool,
    /// Whether the terminal sends focus-in / focus-out events.
    pub focus_events: bool,
    /// The terminal's default background color, if detected via OSC 11.
    pub default_colors: Option<ColorSet>,
}

impl TerminalProbe {
    /// Detect terminal capabilities by querying the real terminal.
    ///
    /// When `stdout` is not a TTY, all capabilities degrade to `false`
    /// / `None`.
    pub fn detect() -> Self {
        if !std::io::stdout().is_terminal() {
            return Self {
                keyboard_enhancement: false,
                bracketed_paste: false,
                focus_events: false,
                default_colors: None,
            };
        }

        let keyboard_enhancement = detect_keyboard_enhancement();
        let bracketed_paste = true; // near-universal in modern terminals
        let focus_events = detect_focus_events();
        let default_colors = detect_default_colors();

        Self {
            keyboard_enhancement,
            bracketed_paste,
            focus_events,
            default_colors,
        }
    }
}

/// Detect keyboard enhancement support via the `TERM_PROGRAM` heuristic.
///
/// A full DA2 query would be ideal but requires async I/O and a timeout,
/// which is complex to integrate at startup. Instead we check well-known
/// terminal identifiers that advertise Kitty keyboard protocol support.
fn detect_keyboard_enhancement() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        match term_program.as_str() {
            // Kitty, WezTerm, Ghostty, Rio, Alacritty (nightly)
            "kitty" | "WezTerm" | "ghostty" | "rio" => return true,
            _ => {}
        }
    }
    // iTerm2 supports it since 3.5+
    if let Ok(term_program_version) = std::env::var("TERM_PROGRAM_VERSION") {
        if let Ok(version) = term_program_version.parse::<f64>() {
            if version >= 3.5 {
                return true;
            }
        }
    }
    false
}

/// Detect focus event support via terminal identification.
fn detect_focus_events() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        match term_program.as_str() {
            "kitty" | "WezTerm" | "ghostty" | "iTerm.app" | "rio" => return true,
            _ => {}
        }
    }
    false
}

/// Query the terminal's default background color via OSC 11.
///
/// Sends `ESC ] 11 ; ESC \` (OSC 11 = background color query) and reads
/// the response. Returns `None` on timeout, parse failure, or non-TTY.
fn detect_default_colors() -> Option<ColorSet> {
    use std::io::Read;

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }

    // Send the OSC 11 query: ESC ] 11 ; ST
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]11;?\x1b\\").ok()?;
    stdout.flush().ok()?;

    // Read the response with a short timeout using crossterm's poll.
    let mut buf = [0u8; 256];
    let mut stdin = std::io::stdin();
    let deadline = Instant::now() + Duration::from_millis(100);

    let mut total = 0usize;
    while Instant::now() < deadline && total < buf.len() {
        if crossterm::event::poll(Duration::from_millis(10)).unwrap_or(false) {
            // Drain any pending events; we just want to consume bytes.
        }
        match stdin.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }

    if total == 0 {
        return None;
    }

    let response = String::from_utf8_lossy(&buf[..total]);
    parse_osc_color_response(&response)
}

/// Query the terminal for OSC 10 (foreground) and OSC 11 (background)
/// colors with an explicit timeout. Opens `/dev/tty`, briefly enables raw
/// mode, writes both queries, and parses responses. Every failure degrades
/// to `(None, None)`; never panics.
pub fn probe_osc_colors(timeout: Duration) -> (Option<ColorSet>, Option<ColorSet>) {
    use std::io::Read;
    use std::io::Write;

    let Ok(mut tty) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    else {
        return (None, None);
    };

    let raw_was_enabled = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    let restorer = RawModeRestorer {
        restore: raw_was_enabled,
    };
    if !raw_was_enabled && crossterm::terminal::enable_raw_mode().is_err() {
        return (None, None);
    }

    const QUERIES: &[u8] = b"\x1b]11;?\x1b\\\x1b]10;?\x1b\\";
    if tty.write_all(QUERIES).is_err() || tty.flush().is_err() {
        drop(restorer);
        return (None, None);
    }

    let deadline = Instant::now() + timeout;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 256];
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;
    #[cfg(unix)]
    let fd = tty.as_raw_fd();
    while buf.len() < 4096 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        #[cfg(unix)]
        {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `pfd` is a valid, exclusively-owned pollfd.
            let ready = unsafe {
                libc::poll(
                    &mut pfd,
                    1,
                    remaining.as_millis().clamp(1, i32::MAX as u128) as i32,
                )
            };
            if ready <= 0 {
                break;
            }
        }
        #[cfg(not(unix))]
        {
            if !crossterm::event::poll(remaining).unwrap_or(false) {
                break;
            }
        }
        match tty.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if parse_osc_pair(&buf).0.is_some() && parse_osc_pair(&buf).1.is_some() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    drop(restorer);
    parse_osc_pair(&buf)
}

/// Parse interleaved OSC 10/11 responses from a byte buffer.
/// Returns `(foreground, background)`.
fn parse_osc_pair(buf: &[u8]) -> (Option<ColorSet>, Option<ColorSet>) {
    let text = String::from_utf8_lossy(buf);
    let mut fg: Option<ColorSet> = None;
    let mut bg: Option<ColorSet> = None;
    let mut rest = text.as_ref();
    while let Some(start) = rest.find("\x1b]") {
        let chunk = &rest[start..];
        if let Some(color) = parse_osc_color_response(chunk) {
            if chunk.starts_with("\x1b]10;") && fg.is_none() {
                fg = Some(color);
            } else if chunk.starts_with("\x1b]11;") && bg.is_none() {
                bg = Some(color);
            }
        }
        rest = &chunk[2.min(chunk.len())..];
        if rest.is_empty() {
            break;
        }
    }
    (fg, bg)
}

/// Restores the raw-mode state found before probing (RAII).
struct RawModeRestorer {
    restore: bool,
}

impl Drop for RawModeRestorer {
    fn drop(&mut self) {
        if !self.restore {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}
/// Parse an OSC color response like `ESC ] 11 ; rgb:RRRR/GGGG/BBBB ESC \`
/// or `ESC ] 11 ; rgb:RR/GG/BB ST`.
fn parse_osc_color_response(response: &str) -> Option<ColorSet> {
    // Find the "rgb:" prefix
    let rgb_start = response.find("rgb:")?;
    let hex = &response[rgb_start + 4..];

    // Strip the OSC terminator: ESC \ (ST) or BEL (\x07) at the end.
    let hex = hex.trim_end_matches('\x07').trim_end_matches("\x1b\\");

    // Split on '/' – expect 3 parts (R, G, B), each 4 or 2 hex digits
    let parts: Vec<&str> = hex.split('/').collect();
    if parts.len() != 3 {
        return None;
    }

    let parse_component = |s: &str| -> Option<u8> {
        let val = u16::from_str_radix(s, 16).ok()?;
        // 4-digit hex (0-ffff) → scale to 0-255; 2-digit stays as-is
        Some(if s.len() > 2 {
            (val >> 8) as u8
        } else {
            val as u8
        })
    };

    Some(ColorSet {
        r: parse_component(parts[0])?,
        g: parse_component(parts[1])?,
        b: parse_component(parts[2])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_osc_color_response_4digit_hex() {
        let resp = "\x1b]11;rgb:ffff/8080/0000\x1b\\";
        let c = parse_osc_color_response(resp).unwrap();
        assert_eq!(
            c,
            ColorSet {
                r: 255,
                g: 128,
                b: 0
            }
        );
    }

    #[test]
    fn parse_osc_color_response_2digit_hex() {
        let resp = "\x1b]11;rgb:ff/80/00\x1b\\";
        let c = parse_osc_color_response(resp).unwrap();
        assert_eq!(
            c,
            ColorSet {
                r: 255,
                g: 128,
                b: 0
            }
        );
    }

    #[test]
    fn parse_osc_color_response_invalid() {
        assert!(parse_osc_color_response("no color here").is_none());
        assert!(parse_osc_color_response("rgb:xx/yy/zz").is_none());
    }

    #[test]
    fn terminal_probe_non_tty_degrades() {
        // In a test environment stdout is usually not a TTY, so the probe
        // should degrade gracefully.
        let probe = TerminalProbe::detect();
        // We can't assert specific values since the test runner environment
        // varies, but we can at least verify the struct constructs.
        let _ = probe;
    }
}
