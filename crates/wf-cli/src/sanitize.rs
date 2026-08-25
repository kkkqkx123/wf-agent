//! User text sanitization for security and correctness.
//!
//! Strips ANSI CSI escape sequences from user-provided text (pasted or
//! external sources) so control bytes cannot corrupt the rendered screen
//! or be misinterpreted by downstream parsers. Newlines and tabs are
//! preserved because they carry layout intent.

/// Strip ANSI CSI escape sequences from `text`, preserving `\n` and `\t`.
///
/// A CSI sequence starts with `ESC [` (0x1b 0x5b) followed by any number
/// of parameter/intermediate bytes (0x30-0x3f / 0x20-0x2f) and a final
/// byte (0x40-0x7e). The entire sequence is removed.
pub fn sanitize_user_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x1b && i + 1 < len && bytes[i + 1] == b'[' {
            // Skip the CSI introducer (ESC [).
            let mut j = i + 2;
            // Skip parameter and intermediate bytes (0x30-0x3f, 0x20-0x2f).
            while j < len && (bytes[j] & 0xe0 == 0x20 || bytes[j] & 0xf0 == 0x30) {
                j += 1;
            }
            // The final byte (0x40-0x7e) terminates the sequence.
            if j < len && bytes[j] & 0x80 == 0 && bytes[j] >= 0x40 {
                j += 1;
            }
            i = j;
            continue;
        }
        // Preserve newlines, tabs and bare ESC; strip other control chars.
        // Multi-byte UTF-8 bytes are all >= 0x80, so they always pass the
        // `b >= 0x20` gate untouched and stay valid UTF-8 in `out`.
        let b = bytes[i];
        if b == b'\n' || b == b'\t' || b >= 0x20 || b == 0x1b {
            out.push(b);
        }
        // Other control bytes (< 0x20, not \n \t ESC) are silently dropped.
        i += 1;
    }
    // Only whole ASCII control bytes were dropped; the remainder is intact
    // UTF-8 (defensive fallback: lossy conversion can never trigger).
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_plain_text() {
        assert_eq!(sanitize_user_text("hello world"), "hello world");
    }

    #[test]
    fn preserves_newlines_and_tabs() {
        assert_eq!(sanitize_user_text("line1\nline2\tend"), "line1\nline2\tend");
    }

    #[test]
    fn strips_csi_color_sequence() {
        assert_eq!(sanitize_user_text("\x1b[31mred\x1b[0m"), "red");
    }

    #[test]
    fn strips_csi_cursor_move() {
        assert_eq!(sanitize_user_text("\x1b[2J\x1b[H"), "");
    }

    #[test]
    fn drops_control_chars_except_newline_tab_esc() {
        // BEL (0x07), BS (0x08), VT (0x0b), FF (0x0c), CR (0x0d) are dropped.
        assert_eq!(sanitize_user_text("a\x07b\x08c\x0bd\x0ce\x0df"), "abcdef");
    }

    #[test]
    fn handles_nested_csi_sequences() {
        assert_eq!(
            sanitize_user_text("\x1b[1;31m\x1b[4mstyled\x1b[0m"),
            "styled"
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(sanitize_user_text(""), "");
    }

    #[test]
    fn preserves_emoji_and_cjk() {
        assert_eq!(sanitize_user_text("你好世界 🎉"), "你好世界 🎉");
    }
}
