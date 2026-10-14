//! Append-only spill file for session output overflow.
//
//! Once a session output buffer passes its spill threshold, the live tail
//! stays in memory while every subsequent chunk is also appended here, so
//! paged reads for absolute positions that fell out of the memory ring can
//! still be served instead of returning only a truncation marker.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Live-tail size that opens the overflow file by default.
pub const DEFAULT_SPILL_THRESHOLD_BYTES: usize = 256_000;

/// Append-only overflow file holding a contiguous absolute byte range of
/// session output. All content is valid UTF-8; arbitrary read ranges are
/// decoded lossily so a page boundary never fails on a split character.
pub(crate) struct SpillFile {
    path: PathBuf,
    base: usize,
    writer: File,
    len: u64,
}

impl SpillFile {
    /// Create the spill file, seeding it with the currently retained live
    /// tail starting at absolute position `base`.
    pub(crate) fn create(path: &Path, base: usize, seed: &str) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        writer.write_all(seed.as_bytes())?;
        writer.flush()?;
        Ok(Self {
            path: path.to_path_buf(),
            base,
            writer,
            len: seed.len() as u64,
        })
    }

    /// Absolute position of the first byte held by the file.
    pub(crate) fn base(&self) -> usize {
        self.base
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// Append a chunk to the file. Best-effort errors propagate so the
    /// caller can fall back to memory-only retention.
    pub(crate) fn append(&mut self, chunk: &str) -> std::io::Result<()> {
        self.writer.write_all(chunk.as_bytes())?;
        self.len += chunk.len() as u64;
        self.writer.flush()?;
        Ok(())
    }

    /// Read up to `max` bytes starting at absolute position `start`.
    /// Returns the text and the absolute end position of the slice.
    pub(crate) fn read_range(&self, start: usize, max: usize) -> std::io::Result<(String, usize)> {
        if max == 0 || start < self.base {
            return Ok((String::new(), start));
        }
        let offset = (start - self.base) as u64;
        if offset >= self.len {
            return Ok((String::new(), start));
        }
        let take = (self.len - offset).min(max as u64) as usize;
        let mut reader = File::open(&self.path)?;
        reader.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; take];
        reader.read_exact(&mut buf)?;
        let end = start + take;
        Ok((String::from_utf8_lossy(&buf).into_owned(), end))
    }

    /// Delete the spill file. Errors are ignored: the file is best-effort
    /// overflow storage and must never fail session teardown.
    pub(crate) fn remove(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
