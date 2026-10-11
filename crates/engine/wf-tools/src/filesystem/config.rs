//! Filesystem tool configuration shared by every file operation.

use std::path::PathBuf;

const DEFAULT_MAX_FILE_SIZE: u64 = 500_000;
const DEFAULT_MAX_CHARS: usize = 50_000;
const DEFAULT_MAX_LINES: usize = 2_000;
const DEFAULT_MAX_RESULTS: usize = 1_000;
pub(super) const DEFAULT_GREP_MAX_RESULTS: usize = 300;
pub(super) const DEFAULT_GLOB_MAX_RESULTS: usize = 50;

#[derive(Debug, Clone)]
pub struct FsToolConfig {
    /// Base directory for relative paths; absolute paths are used as-is.
    pub workspace_dir: Option<PathBuf>,
    pub max_file_size: u64,
    pub max_chars: usize,
    pub max_lines: usize,
    pub max_results: usize,
    pub enable_ignore: bool,
    /// Scope enforcement mode following the sandbox contract: Strict
    /// rejects out-of-scope paths, Lenient warns and still executes (the
    /// checkpoint layer filters the write and reports it as out of scope),
    /// Disabled skips scope gating. `None` defaults to Strict.
    pub scope_mode: Option<wf_types::script::sandbox::SandboxMode>,
}

impl Default for FsToolConfig {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_chars: DEFAULT_MAX_CHARS,
            max_lines: DEFAULT_MAX_LINES,
            max_results: DEFAULT_MAX_RESULTS,
            enable_ignore: true,
            scope_mode: None,
        }
    }
}
