//! UTF-8 chunk decoding with cross-chunk carry for the session output
//! readers.
//!
//! [`Utf8ChunkDecoder`] decodes raw pipe bytes chunk by chunk while carrying
//! the incomplete tail of a multi-byte UTF-8 sequence across reads, so a
//! character split at a chunk boundary is never corrupted by per-chunk lossy
//! decoding (at most 3 bytes are carried). The PTY path additionally
//! normalizes CRLF on the raw bytes before decoding, keeping the cross-chunk
//! `\r\n` state here as well.

/// Decodes raw pipe bytes chunk by chunk while carrying the incomplete tail of
/// a multi-byte UTF-8 sequence across reads, so a character split at a chunk
/// boundary is never corrupted by per-chunk lossy decoding (at most 3 bytes
/// are carried). The PTY path additionally normalizes CRLF on the raw bytes
/// before decoding, keeping the cross-chunk `\r\n` state here as well.
pub(crate) struct Utf8ChunkDecoder {
    carry: Vec<u8>,
    normalize_crlf: bool,
    pending_cr: bool,
}

impl Utf8ChunkDecoder {
    pub(crate) fn new(normalize_crlf: bool) -> Self {
        Self {
            carry: Vec::new(),
            normalize_crlf,
            pending_cr: false,
        }
    }

    /// Feed a raw chunk; returns the decodable prefix. Incomplete trailing
    /// UTF-8 bytes (and, on the PTY path, a trailing `\r`) are retained for
    /// the next call.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> String {
        let mut combined = std::mem::take(&mut self.carry);
        combined.extend_from_slice(chunk);
        if self.normalize_crlf {
            combined = normalize_crlf(&combined, &mut self.pending_cr);
        }
        match std::str::from_utf8(&combined) {
            Ok(_) => {
                self.carry.clear();
                String::from_utf8_lossy(&combined).into_owned()
            }
            Err(e) => match e.error_len() {
                // Truncated sequence at the very end: keep it for the next
                // chunk so the split character is decoded intact.
                None => {
                    let valid = e.valid_up_to();
                    let decoded = String::from_utf8_lossy(&combined[..valid]).into_owned();
                    self.carry = combined[valid..].to_vec();
                    decoded
                }
                // Genuinely invalid byte(s) mid-stream: lossy-decode the whole
                // batch and drop the carry.
                Some(_) => {
                    self.carry.clear();
                    String::from_utf8_lossy(&combined).into_owned()
                }
            },
        }
    }

    /// Emit the remaining carried bytes on EOF (an incomplete trailing
    /// sequence or a pending `\r` is decoded/emitted lossily).
    pub(crate) fn flush(&mut self) -> String {
        let bytes = std::mem::take(&mut self.carry);
        if self.normalize_crlf {
            let normalized = normalize_crlf(&bytes, &mut self.pending_cr);
            return String::from_utf8_lossy(&normalized).into_owned();
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// Normalize CRLF (and a split `\r` / `\n` across chunks) to `\n` on raw
/// bytes. Operates on bytes so it composes with the UTF-8 chunk decoding done
/// by [`Utf8ChunkDecoder`].
fn normalize_crlf(bytes: &[u8], pending_cr: &mut bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    if *pending_cr {
        if bytes.first() == Some(&b'\n') {
            // A `\r\n` split across two chunks becomes a single `\n`.
            out.push(b'\n');
            i = 1;
        } else {
            out.push(b'\r');
        }
        *pending_cr = false;
    }
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                out.push(b'\n');
                i += 2;
            } else if i + 1 == bytes.len() {
                *pending_cr = true;
                i += 1;
            } else {
                out.push(b'\r');
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_crlf() {
        let mut pending = false;
        assert_eq!(
            normalize_crlf(b"a\r\nb\nc\r", &mut pending).as_slice(),
            b"a\nb\nc"
        );
        assert_eq!(normalize_crlf(b"\nd", &mut pending).as_slice(), b"\nd");
        assert_eq!(normalize_crlf(b"plain", &mut pending).as_slice(), b"plain");
    }

    #[test]
    fn test_utf8_chunk_decoder_split_sequence() {
        // 中 = E4 B8 AD, split across two chunks; must not be corrupted.
        let mut d = Utf8ChunkDecoder::new(false);
        assert_eq!(d.push(&[0xE4, 0xB8]), "");
        assert_eq!(d.push(&[0xAD, b'X', b'\n', b'Y', b'\n']), "中X\nY\n");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn test_utf8_chunk_decoder_partial_line() {
        let mut d = Utf8ChunkDecoder::new(false);
        assert_eq!(d.push(b"he"), "he");
        assert_eq!(d.push(b"llo"), "llo");
        assert_eq!(d.flush(), "");
    }

    #[test]
    fn test_utf8_chunk_decoder_invalid_bytes() {
        let mut d = Utf8ChunkDecoder::new(false);
        assert_eq!(d.push(&[0xFF, b'a', b'\n']), "\u{FFFD}a\n");
        // Carry must not grow past the incomplete sequence.
        assert_eq!(d.push(b"b"), "b");
        assert!(d.carry.is_empty());
    }

    #[test]
    fn test_utf8_chunk_decoder_crlf_across_chunks() {
        let mut d = Utf8ChunkDecoder::new(true);
        assert_eq!(d.push(b"a\r"), "a");
        assert_eq!(d.push(b"\nb"), "\nb");
        assert_eq!(d.flush(), "");
        // A trailing lone \r is emitted on EOF.
        let mut d = Utf8ChunkDecoder::new(true);
        assert_eq!(d.push(b"x\r"), "x");
        assert_eq!(d.flush(), "\r");
    }
}
