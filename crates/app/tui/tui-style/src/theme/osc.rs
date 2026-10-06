//! OSC 10/11 color response parsing.
//!
//! Incremental parser for OSC 10/11 color responses
//! (`ESC ] 11 ; rgb:RRRR/GGGG/BBBB ST`, ST = `ESC \` or BEL). Non-OSC bytes
//! are dropped, so responses split across arbitrary chunk boundaries still
//! parse.

use super::data::Rgb;

/// Incremental parser for OSC 10/11 color responses.
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
    } else {
        let r = rest.strip_prefix(b"11;")?;
        (11u8, r)
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
