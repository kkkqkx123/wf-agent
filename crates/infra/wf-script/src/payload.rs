use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Maximum accepted standard input size (1 MiB). Larger payloads must travel
/// as `input_files` references instead of inline text.
pub const MAX_STDIN_BYTES: usize = 1_048_576;

/// Maximum total environment size (256 KiB). Guards against OS `ARG_MAX`/env
/// limits and quoting blowups when callers smuggle payloads through env vars.
pub const MAX_ENV_BYTES: usize = 262_144;

/// Byte size of an environment map (keys, values and separators).
pub(crate) fn total_env_bytes(env: &HashMap<String, String>) -> usize {
    env.iter().map(|(k, v)| k.len() + v.len() + 2).sum()
}

/// Keep the tail of `text` within `max_bytes`, cutting on a UTF-8 character
/// boundary. Returns the kept text and whether anything was dropped.
pub fn truncate_tail(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let cut = text.len() - max_bytes;
    let boundary = text.floor_char_boundary(cut);
    (text[boundary..].to_string(), true)
}

/// Validate a file-typed argument value: non-empty, no NUL bytes, must name
/// an existing file, and must stay inside `workdir` when one is set.
/// Returns the validated path unchanged. This is a best-effort pre-check at
/// render time; the transport owns the final open so a rename between check
/// and use is still confined by the sandbox and session workdir.
pub(crate) fn validate_file_arg(path: &str, workdir: Option<&str>) -> Result<String, String> {
    if path.is_empty() {
        return Err("file argument must not be empty".to_string());
    }
    if path.contains('\0') {
        return Err(format!("file argument '{path}' contains a NUL byte"));
    }
    let candidate = Path::new(path);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        match workdir {
            Some(root) => Path::new(root).join(candidate),
            None => std::env::current_dir()
                .map(|cwd| cwd.join(candidate))
                .map_err(|e| format!("cannot resolve relative file argument: {e}"))?,
        }
    };
    let canonical_file = absolute
        .canonicalize()
        .map_err(|_| format!("file argument '{path}' does not name an existing file"))?;
    if !canonical_file.is_file() {
        return Err(format!("file argument '{path}' is not a regular file"));
    }
    if let Some(root) = workdir {
        let canonical_root = Path::new(root)
            .canonicalize()
            .map_err(|_| format!("working directory '{root}' does not exist"))?;
        if !canonical_file.starts_with(&canonical_root) {
            return Err(format!(
                "file argument '{path}' escapes the working directory"
            ));
        }
    }
    Ok(path.to_string())
}

/// Spill a full stream to `dir` under `file_name`, creating the directory.
/// Returns the written path.
pub(crate) fn spill_output(dir: &Path, file_name: &str, content: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file_name);
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Capped stream ready to place on a result: the kept tail, whether the head
/// was dropped, the full byte size, an optional spill path holding the
/// complete content, and an optional spill failure note. A spill failure
/// never drops the kept tail; it is reported so callers do not assume a
/// missing file is still readable.
pub struct CappedStream {
    pub text: Option<String>,
    pub truncated: bool,
    pub total_bytes: u64,
    pub spilled_path: Option<String>,
    pub spill_error: Option<String>,
}

/// Apply `max_output_bytes` to one stream. A `None` cap keeps the historical
/// unbounded behavior. When truncated and `spill_dir` is set, the complete
/// stream is written to `<spill_dir>/<stream_name>.txt`. A spill write
/// failure is returned as `spill_error` instead of being dropped.
pub fn cap_stream(
    text: Option<String>,
    max_bytes: Option<u64>,
    spill_dir: Option<&str>,
    stream_name: &str,
) -> CappedStream {
    let Some(content) = text else {
        return CappedStream {
            text: None,
            truncated: false,
            total_bytes: 0,
            spilled_path: None,
            spill_error: None,
        };
    };
    let total_bytes = content.len() as u64;
    let Some(cap) = max_bytes else {
        return CappedStream {
            text: Some(content),
            truncated: false,
            total_bytes,
            spilled_path: None,
            spill_error: None,
        };
    };
    let (kept, truncated) = truncate_tail(&content, cap as usize);
    let (spilled_path, spill_error) = if truncated {
        match spill_dir {
            Some(dir) => {
                match spill_output(Path::new(dir), &format!("{stream_name}.txt"), &content) {
                    Ok(p) => (Some(p.to_string_lossy().to_string()), None),
                    Err(e) => (
                        None,
                        Some(format!("cannot spill {stream_name} to '{dir}': {e}")),
                    ),
                }
            }
            None => (None, None),
        }
    } else {
        (None, None)
    };
    CappedStream {
        text: Some(kept),
        truncated,
        total_bytes,
        spilled_path,
        spill_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_tail_keeps_short_text() {
        let (kept, truncated) = truncate_tail("hello", 100);
        assert_eq!(kept, "hello");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_tail_keeps_tail_on_char_boundary() {
        let text = format!("{}tail", "界".repeat(100));
        let (kept, truncated) = truncate_tail(&text, 8);
        assert!(truncated);
        assert!(kept.ends_with("tail"));
        assert!(std::str::from_utf8(kept.as_bytes()).is_ok());
    }

    #[test]
    fn test_validate_file_arg_rejects_missing_and_escape() {
        assert!(validate_file_arg("", None).is_err());
        assert!(validate_file_arg("/no/such/file-xyz", None).is_err());
        let dir = std::env::temp_dir().join("wf-payload-test");
        std::fs::create_dir_all(&dir).expect("test dir is creatable");
        let inner = dir.join("inner.txt");
        std::fs::write(&inner, "data").expect("test file is writable");
        let root = dir.to_string_lossy().to_string();
        assert!(validate_file_arg(&inner.to_string_lossy(), Some(&root)).is_ok());
        assert!(validate_file_arg("/etc/hostname", Some(&root)).is_err());
    }

    #[test]
    fn test_cap_stream_spills_full_content() {
        let dir = std::env::temp_dir().join("wf-payload-spill");
        let capped = cap_stream(
            Some("0123456789".to_string()),
            Some(4),
            Some(&dir.to_string_lossy()),
            "probe-stdout",
        );
        assert!(capped.truncated);
        assert_eq!(capped.text.as_deref(), Some("6789"));
        assert_eq!(capped.total_bytes, 10);
        let spilled = capped.spilled_path.expect("spill path is reported");
        assert_eq!(
            std::fs::read_to_string(spilled).expect("spill is readable"),
            "0123456789"
        );
    }
}
