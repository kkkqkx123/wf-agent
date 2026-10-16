//! Terminal image emission: pure escape-sequence builders.
//!
//! Detection lives in [`crate::capabilities`]; this module only builds the
//! bytes for an already-chosen protocol. Every builder is a pure function
//! over its inputs so unit tests assert exact wire bytes without a terminal.
//! Multiplexed sessions stay on placeholders: callers map any multiplexed
//! state to [`crate::capabilities::ImageProtocol::None`] before selecting,
//! and each protocol additionally carries an independent switch in
//! [`ImageEmitConfig`] that fails back to the text placeholder. All three
//! switches default off, so default output is byte-identical to the
//! placeholder path.

use crate::capabilities::ImageProtocol;

/// Base64 characters per Kitty APC chunk. Matches the safety margin used by
/// the reference implementation: small enough for conservative terminals.
pub const KITTY_CHUNK_SIZE: usize = 4096;

/// Independent per-protocol emission switches. Each step is separately
/// gated so a product can enable one transport at a time; any disabled or
/// failed protocol falls back to the text placeholder with unchanged
/// geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageEmitConfig {
    /// Emit Kitty graphics (`APC G`) sequences.
    pub kitty: bool,
    /// Emit iTerm2 inline image (`OSC 1337`) sequences.
    pub iterm2: bool,
    /// Emit Sixel via the external image toolchain.
    pub sixel: bool,
}

impl Default for ImageEmitConfig {
    /// Default off: placeholders only, output matches the pre-backend state.
    fn default() -> Self {
        Self {
            kitty: false,
            iterm2: false,
            sixel: false,
        }
    }
}

impl ImageEmitConfig {
    /// All transports enabled (explicit product opt-in).
    pub fn all_enabled() -> Self {
        Self {
            kitty: true,
            iterm2: true,
            sixel: true,
        }
    }

    /// True when `protocol` is enabled by this config.
    pub fn enabled_for(self, protocol: ImageProtocol) -> bool {
        match protocol {
            ImageProtocol::None => false,
            ImageProtocol::Kitty => self.kitty,
            ImageProtocol::Iterm2 => self.iterm2,
            ImageProtocol::Sixel => self.sixel,
        }
    }
}

/// Tmux passthrough brackets: multiplexers swallow unknown OSC/APC unless
/// wrapped in the passthrough form with doubled escapes.
pub fn tmux_brackets(is_tmux: bool) -> (&'static str, &'static str, &'static str) {
    if is_tmux {
        ("\x1bPtmux;", "\x1b\x1b", "\x1b\\")
    } else {
        ("", "\x1b", "")
    }
}

/// Pure tmux detection over explicit inputs so tests never touch the
/// environment. True when the multiplexer variable is set and non-blank or
/// the terminal name starts with `tmux`.
pub fn detect_tmux(term: Option<&str>, tmux_var: Option<&str>) -> bool {
    if tmux_var.is_some_and(|value| !value.trim().is_empty()) {
        return true;
    }
    term.is_some_and(|term| term.starts_with("tmux"))
}

/// Live tmux detection from the process environment.
pub fn is_tmux() -> bool {
    detect_tmux(
        std::env::var("TERM").ok().as_deref(),
        std::env::var("TMUX").ok().as_deref(),
    )
}

/// Gate a detected protocol by the emission switches and the multiplexer
/// state. Multiplexed sessions always resolve to none: without pass-through
/// proof graphics bytes would corrupt the shared stream.
pub fn select_protocol(
    detected: ImageProtocol,
    config: &ImageEmitConfig,
    is_tmux: bool,
) -> ImageProtocol {
    if is_tmux {
        return ImageProtocol::None;
    }
    if config.enabled_for(detected) {
        detected
    } else {
        ImageProtocol::None
    }
}

/// True when an emission may touch stdout: a real protocol plus a TTY.
/// Redirected output disables every protocol so headless commands never
/// emit graphics into captured streams.
pub fn can_emit(protocol: ImageProtocol, stdout_is_terminal: bool) -> bool {
    stdout_is_terminal && protocol.is_available()
}

/// Minimal standard-base64 encoder (no extra dependency for one call site).
pub fn encode_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().saturating_add(2) / 3 * 4);
    for chunk in data.chunks(3) {
        let a = u32::from(chunk[0]);
        let b = if chunk.len() > 1 {
            u32::from(chunk[1])
        } else {
            0
        };
        let c = if chunk.len() > 2 {
            u32::from(chunk[2])
        } else {
            0
        };
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Build a Kitty transmit-and-display sequence for an already-encoded
/// payload: `\x1b_Ga=T,f=100,c=cols,r=rows,m=;payload\x1b\\`, chunked so no
/// APC exceeds [`KITTY_CHUNK_SIZE`] characters. The first chunk carries the
/// display geometry; continuation chunks carry only the `m` flag.
pub fn kitty_sequence(encoded: &str, cols: u16, rows: u16, is_tmux: bool) -> String {
    let (start, escape, end) = tmux_brackets(is_tmux);
    let cols = cols.max(1);
    let rows = rows.max(1);
    let bytes = encoded.as_bytes();
    let chunks: Vec<&str> = if bytes.is_empty() {
        vec![""]
    } else {
        bytes
            .chunks(KITTY_CHUNK_SIZE)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect()
    };
    let last = chunks.len().saturating_sub(1);
    let mut out = String::new();
    out.push_str(start);
    for (index, chunk) in chunks.iter().enumerate() {
        let more = if index == last { 0 } else { 1 };
        out.push_str(escape);
        if index == 0 {
            out.push_str(&format!("_Ga=T,f=100,c={cols},r={rows},m={more};{chunk}"));
        } else {
            out.push_str(&format!("_Gm={more};{chunk}"));
        }
        out.push_str(escape);
        out.push('\\');
    }
    out.push_str(end);
    out
}

/// Build an iTerm2 inline-image sequence for an already-encoded payload:
/// `OSC 1337;File=name=..;size=..;inline=1;width=..;height=..;
/// preserveAspectRatio=1:payload BEL`. Both cell dimensions are always
/// explicit so the terminal reserves exactly the rows the placeholder
/// measured; inside tmux the whole sequence uses passthrough wrapping.
pub fn iterm2_sequence(
    filename_b64: &str,
    byte_len: usize,
    cols: u16,
    rows: u16,
    encoded: &str,
    is_tmux: bool,
) -> String {
    let (start, escape, end) = tmux_brackets(is_tmux);
    let cols = cols.max(1);
    let rows = rows.max(1);
    format!(
        "{start}{escape}]1337;File=name={filename_b64};size={byte_len};inline=1;width={cols};height={rows};preserveAspectRatio=1:{encoded}\x07{end}"
    )
}

/// External toolchain argv converting `path` to Sixel bounded by the pixel
/// box. Pure argv construction: the caller executes it and any failure
/// falls back to the text placeholder.
pub fn sixel_command(path: &str, pixel_width: u32, pixel_height: u32) -> Vec<String> {
    vec![
        "convert".to_string(),
        path.to_string(),
        "-geometry".to_string(),
        format!("{}x{}>", pixel_width.max(1), pixel_height.max(1)),
        "-colors".to_string(),
        "256".to_string(),
        "sixel:-".to_string(),
    ]
}

/// Wrap a raw Sixel payload for tmux passthrough: inner escapes double,
/// outer DCS markers frame the whole payload. Non-tmux output is identity.
pub fn wrap_sixel_passthrough(sixel_payload: &str, is_tmux: bool) -> String {
    if !is_tmux {
        return sixel_payload.to_string();
    }
    let (start, _, end) = tmux_brackets(true);
    let escaped = sixel_payload.replace('\x1b', "\x1b\x1b");
    format!("{start}{escaped}{end}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_disables_every_protocol() {
        let config = ImageEmitConfig::default();
        for protocol in [
            ImageProtocol::None,
            ImageProtocol::Kitty,
            ImageProtocol::Iterm2,
            ImageProtocol::Sixel,
        ] {
            assert_eq!(
                select_protocol(protocol, &config, false),
                ImageProtocol::None,
                "default must stay on placeholders for {protocol:?}"
            );
        }
    }

    #[test]
    fn per_protocol_switches_gate_independently() {
        let kitty_only = ImageEmitConfig {
            kitty: true,
            ..ImageEmitConfig::default()
        };
        assert_eq!(
            select_protocol(ImageProtocol::Kitty, &kitty_only, false),
            ImageProtocol::Kitty
        );
        assert_eq!(
            select_protocol(ImageProtocol::Iterm2, &kitty_only, false),
            ImageProtocol::None
        );
        assert_eq!(
            select_protocol(ImageProtocol::Sixel, &kitty_only, false),
            ImageProtocol::None
        );
    }

    #[test]
    fn multiplexed_sessions_stay_on_placeholders() {
        let config = ImageEmitConfig::all_enabled();
        for protocol in [
            ImageProtocol::Kitty,
            ImageProtocol::Iterm2,
            ImageProtocol::Sixel,
        ] {
            assert_eq!(
                select_protocol(protocol, &config, true),
                ImageProtocol::None,
                "tmux must never emit graphics without pass-through proof"
            );
        }
    }

    #[test]
    fn redirected_stdout_disables_every_protocol() {
        for protocol in [
            ImageProtocol::Kitty,
            ImageProtocol::Iterm2,
            ImageProtocol::Sixel,
            ImageProtocol::None,
        ] {
            assert!(!can_emit(protocol, false));
        }
        assert!(can_emit(ImageProtocol::Kitty, true));
        assert!(!can_emit(ImageProtocol::None, true));
    }

    #[test]
    fn base64_vectors_match_standard_alphabet() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"name"), encode_base64(b"name"));
    }

    #[test]
    fn kitty_single_chunk_carries_geometry() {
        let payload = encode_base64(b"bytes");
        let seq = kitty_sequence(&payload, 40, 12, false);
        assert!(seq.starts_with("\x1b_Ga=T,f=100,c=40,r=12,m=0;"));
        assert!(seq.ends_with("\x1b\\"));
        assert!(seq.contains(&payload));
    }

    #[test]
    fn kitty_long_payload_chunks_with_continuation_flags() {
        let big = "A".repeat(KITTY_CHUNK_SIZE * 2 + 10);
        let seq = kitty_sequence(&big, 40, 12, false);
        assert!(seq.contains("c=40,r=12,m=1;"));
        assert!(seq.contains("_Gm=1;") || seq.contains("_Gm=0;"));
        assert!(seq.ends_with("\x1b\\"));
        let first = seq.find("m=1;").expect("first chunk continues");
        let last = seq.rfind("m=0;").expect("final chunk terminates");
        assert!(first < last);
    }

    #[test]
    fn kitty_tmux_passthrough_wraps_and_doubles_escapes() {
        let seq = kitty_sequence("QUJD", 4, 2, true);
        assert!(seq.starts_with("\x1bPtmux;\x1b\x1b_Ga=T"), "{seq:?}");
        assert!(seq.ends_with("\x1b\\"), "{seq:?}");
    }

    #[test]
    fn iterm2_payload_sets_both_cell_dimensions() {
        let seq = iterm2_sequence("bmFtZQ==", 1234, 40, 12, "QUJD", false);
        assert!(seq.starts_with("\x1b]1337;File="), "{seq:?}");
        assert!(seq.contains("size=1234"), "{seq:?}");
        assert!(seq.contains("width=40;height=12"), "{seq:?}");
        assert!(seq.contains("preserveAspectRatio=1"), "{seq:?}");
        assert!(seq.ends_with("QUJD\x07"), "{seq:?}");
    }

    #[test]
    fn iterm2_tmux_passthrough_wraps_sequence() {
        let seq = iterm2_sequence("bmFtZQ==", 1, 4, 2, "QQ==", true);
        assert!(seq.starts_with("\x1bPtmux;\x1b\x1b]1337;"), "{seq:?}");
        assert!(seq.ends_with("\x07\x1b\\"), "{seq:?}");
    }

    #[test]
    fn sixel_command_targets_bounded_toolchain_call() {
        let argv = sixel_command("diagram.png", 640, 480);
        assert_eq!(
            argv,
            vec![
                "convert",
                "diagram.png",
                "-geometry",
                "640x480>",
                "-colors",
                "256",
                "sixel:-",
            ]
        );
    }

    #[test]
    fn sixel_passthrough_is_identity_off_tmux() {
        let payload = "\x1bPq...sixel...\x1b\\";
        assert_eq!(wrap_sixel_passthrough(payload, false), payload);
        let wrapped = wrap_sixel_passthrough(payload, true);
        assert!(wrapped.starts_with("\x1bPtmux;"), "{wrapped:?}");
        assert!(wrapped.ends_with("\x1b\\"), "{wrapped:?}");
        assert!(wrapped.contains("\x1b\x1bPq"), "{wrapped:?}");
    }

    #[test]
    fn tmux_detection_uses_explicit_inputs() {
        assert!(detect_tmux(
            Some("xterm-256color"),
            Some("/tmp/tmux-1/default,2,0")
        ));
        assert!(detect_tmux(Some("tmux-256color"), None));
        assert!(!detect_tmux(Some("xterm-256color"), None));
        assert!(!detect_tmux(Some("xterm-256color"), Some("  ")));
    }
}
