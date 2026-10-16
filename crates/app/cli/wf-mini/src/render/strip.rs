//! Terminal-escape stripping for model output.

/// Strip terminal escapes from model text so it can never drive the
/// terminal. CSI (`ESC [ ... final`) and OSC (`ESC ] ... BEL/ST`) sequences
/// are removed whole; a trailing partial sequence is dropped with the delta
/// that carries it. Other C0 controls (including bare `ESC`, `CR`, `DEL`)
/// are dropped; `\n` and `\t` survive and multibyte UTF-8 passes through
/// untouched.
pub fn strip_terminal_escapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x1b {
            if i + 1 < len && bytes[i + 1] == b'[' {
                let mut j = i + 2;
                while j < len && is_csi_body(bytes[j]) {
                    j += 1;
                }
                if j < len && bytes[j] >= 0x40 && bytes[j] <= 0x7e {
                    j += 1;
                }
                i = j;
                continue;
            }
            if i + 1 < len && bytes[i + 1] == b']' {
                let mut j = i + 2;
                while j < len {
                    if bytes[j] == 0x07 {
                        j += 1;
                        break;
                    }
                    if bytes[j] == 0x1b && j + 1 < len && bytes[j + 1] == b'\\' {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // Bare ESC (or an unknown sequence leader): drop the byte
            // itself. Any payload without its introducer is inert text.
            i += 1;
            continue;
        }
        let b = bytes[i];
        if b == b'\n' || b == b'\t' || b >= 0x20 && b != 0x7f {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

fn is_csi_body(b: u8) -> bool {
    (0x30..=0x3f).contains(&b) || (0x20..=0x2f).contains(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_color_and_cursor_sequences() {
        assert_eq!(strip_terminal_escapes("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_terminal_escapes("a\x1b[2J\x1b[Hb"), "ab");
    }

    #[test]
    fn strips_osc_hyperlink_keeping_text() {
        assert_eq!(
            strip_terminal_escapes("\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\"),
            "link"
        );
        assert_eq!(strip_terminal_escapes("a\x1b]0;title\x07b"), "ab");
    }

    #[test]
    fn drops_bare_esc_and_controls_but_keeps_text() {
        assert_eq!(strip_terminal_escapes("a\x1bb"), "ab");
        assert_eq!(strip_terminal_escapes("a\rb\nc\td"), "ab\nc\td");
        assert_eq!(strip_terminal_escapes("你好\x1b[31m世界"), "你好世界");
    }

    #[test]
    fn drops_trailing_partial_sequence() {
        assert_eq!(strip_terminal_escapes("abc\x1b["), "abc");
        assert_eq!(strip_terminal_escapes("abc\x1b[31"), "abc");
    }
}
