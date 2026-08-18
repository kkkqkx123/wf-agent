use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use wf_types::config::file_checkpoint::FailureBehavior;

use crate::error::CheckpointError;
use crate::file::{sha256_hex, FileState};

/// Hardcoded ignore names.
/// Any directory (or file) with these names is excluded from scanning at any
/// depth, and restore never deletes them.
pub const HARDCODED_IGNORE_DIRS: &[&str] = &[".git", "node_modules"];

/// Configuration for workspace scanning.
#[derive(Debug, Clone, Default)]
pub struct ScanConfig {
    /// Additional ignore patterns (glob syntax, `*` does not cross `/`).
    pub custom_ignore_patterns: Vec<String>,
    /// Per-file error handling: `Error` propagates, `Warn` logs and skips,
    /// `Ignore` skips silently.
    pub failure_behavior: FailureBehavior,
}

/// Result of a full workspace scan.
#[derive(Debug, Clone)]
pub struct WorkspaceScan {
    /// Hashed file states with workspace-relative paths.
    pub files: Vec<FileState>,
    /// All visited directories (relative, posix separators), excluding
    /// ignored ones.
    pub dirs: Vec<String>,
    /// Directories that contain no files (recreated on restore).
    pub empty_dirs: Vec<String>,
}

/// Recursive workspace scanner: enumerates files, applies ignore rules
/// (hardcoded + per-directory `.gitignore` + custom patterns) and computes
/// content hashes.
pub struct WorkspaceScanner {
    config: ScanConfig,
    matcher: GlobSet,
}

fn build_glob(pattern: &str) -> Option<globset::Glob> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .ok()
}

/// Add an ignore pattern to the globset.
///
/// Pattern semantics:
/// - a trailing `/` marks a directory prefix,
/// - patterns without `/` also match the bare name at any depth,
/// - `*`/`?` do not cross path separators.
fn add_pattern(builder: &mut GlobSetBuilder, raw: &str) {
    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() {
        return;
    }
    if trimmed.contains('/') {
        // Anchored at the workspace root: exact path and everything below.
        if let Some(g) = build_glob(trimmed) {
            builder.add(g);
        }
        if let Some(g) = build_glob(&format!("{trimmed}/**")) {
            builder.add(g);
        }
    } else {
        // Bare name: root-level prefix plus any-depth name match.
        for pattern in [
            trimmed.to_string(),
            format!("{trimmed}/**"),
            format!("**/{trimmed}"),
            format!("**/{trimmed}/**"),
        ] {
            if let Some(g) = build_glob(&pattern) {
                builder.add(g);
            }
        }
    }
}

/// Matcher for the hardcoded ignore names only (used by restore to protect
/// `.git` / `node_modules` from deletion).
pub fn hardcoded_ignore_matcher() -> &'static GlobSet {
    static MATCHER: OnceLock<GlobSet> = OnceLock::new();
    MATCHER.get_or_init(|| {
        let mut builder = GlobSetBuilder::new();
        for dir in HARDCODED_IGNORE_DIRS {
            add_pattern(&mut builder, dir);
        }
        builder
            .build()
            .unwrap_or_else(|_| GlobSetBuilder::new().build().expect("empty globset"))
    })
}

/// Whether a workspace-relative path is protected by the hardcoded ignore
/// rules (`.git` / `node_modules`).
pub fn is_hardcoded_ignored(relative_path: &str) -> bool {
    hardcoded_ignore_matcher().is_match(relative_path)
}

impl WorkspaceScanner {
    pub fn new(config: ScanConfig) -> Self {
        let mut builder = GlobSetBuilder::new();
        for dir in HARDCODED_IGNORE_DIRS {
            add_pattern(&mut builder, dir);
        }
        for pattern in &config.custom_ignore_patterns {
            add_pattern(&mut builder, pattern);
        }
        let matcher = builder
            .build()
            .unwrap_or_else(|_| GlobSetBuilder::new().build().expect("empty globset"));
        Self { config, matcher }
    }

    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    /// Whether a workspace-relative path matches any ignore rule.
    pub fn is_ignored(&self, relative_path: &str) -> bool {
        self.matcher.is_match(relative_path)
    }

    /// Scan the workspace root recursively and return hashed file states,
    /// visited directories and empty directories. Per-directory `.gitignore`
    /// files are collected upfront and combined with the hardcoded and custom
    /// patterns.
    pub fn scan(&self, root: &Path) -> Result<WorkspaceScan, CheckpointError> {
        let mut gitignore_patterns = Vec::new();
        self.collect_gitignore(root, root, &mut gitignore_patterns)?;
        let mut builder = GlobSetBuilder::new();
        for dir in HARDCODED_IGNORE_DIRS {
            add_pattern(&mut builder, dir);
        }
        for pattern in &gitignore_patterns {
            add_pattern(&mut builder, pattern);
        }
        for pattern in &self.config.custom_ignore_patterns {
            add_pattern(&mut builder, pattern);
        }
        let matcher = builder
            .build()
            .unwrap_or_else(|_| GlobSetBuilder::new().build().expect("empty globset"));

        let mut files = Vec::new();
        let mut dirs = Vec::new();
        self.scan_dir(root, root, &matcher, &mut files, &mut dirs)?;
        let empty_dirs = find_empty_dirs(&dirs, &files);
        Ok(WorkspaceScan {
            files,
            dirs,
            empty_dirs,
        })
    }

    /// Read every `.gitignore` under the workspace, prefixing each pattern
    /// line with its directory relative to the root.
    /// `.git`/`node_modules` directories are not traversed.
    fn collect_gitignore(
        &self,
        root: &Path,
        current: &Path,
        patterns: &mut Vec<String>,
    ) -> Result<(), CheckpointError> {
        let gitignore_path = current.join(".gitignore");
        if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
            let prefix = current
                .strip_prefix(root)
                .map(|rel| rel.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let prefixed = if prefix.is_empty() {
                    trimmed.to_string()
                } else {
                    format!("{prefix}/{trimmed}")
                };
                patterns.push(prefixed);
            }
        }

        let entries = match fs::read_dir(current) {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        let mut entries: Vec<_> = match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if HARDCODED_IGNORE_DIRS.contains(&name.as_ref()) {
                continue;
            }
            self.collect_gitignore(root, &entry.path(), patterns)?;
        }
        Ok(())
    }

    fn scan_dir(
        &self,
        root: &Path,
        current: &Path,
        matcher: &GlobSet,
        files: &mut Vec<FileState>,
        dirs: &mut Vec<String>,
    ) -> Result<(), CheckpointError> {
        let entries = match fs::read_dir(current) {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        let mut entries: Vec<_> = match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(err) => {
                return self.handle_failure(
                    &format!("failed to read directory '{}'", current.display()),
                    &err,
                );
            }
        };
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(err) => {
                    return self.handle_failure(
                        &format!("failed to stat '{}'", entry.path().display()),
                        &err,
                    );
                }
            };
            let relative = match path_to_relative(root, &entry.path()) {
                Some(rel) => rel,
                None => continue,
            };
            if matcher.is_match(&relative) {
                continue;
            }

            if file_type.is_dir() {
                dirs.push(relative);
                self.scan_dir(root, &entry.path(), matcher, files, dirs)?;
            } else if file_type.is_file() {
                match self.hash_file(&relative, &entry.path()) {
                    Ok(state) => files.push(state),
                    Err(CheckpointError::Io(err)) => {
                        self.handle_failure(&format!("failed to hash '{}'", relative), &err)?;
                    }
                    Err(err) => return Err(err),
                }
            }
        }
        Ok(())
    }

    fn hash_file(&self, relative: &str, absolute: &Path) -> Result<FileState, CheckpointError> {
        let content = fs::read(absolute)?;
        let metadata = fs::metadata(absolute)?;
        let last_modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Ok(FileState {
            path: relative.to_string(),
            hash: sha256_hex(&content),
            size: metadata.len(),
            last_modified,
            deleted: false,
        })
    }

    fn handle_failure(&self, ctx: &str, err: &std::io::Error) -> Result<(), CheckpointError> {
        match self.config.failure_behavior {
            FailureBehavior::Error => Err(CheckpointError::Io(std::io::Error::other(format!(
                "{ctx}: {err}"
            )))),
            FailureBehavior::Warn => {
                tracing::warn!("{ctx}: {err}");
                Ok(())
            }
            FailureBehavior::Ignore => Ok(()),
        }
    }
}

/// Compute the workspace-relative posix path of an entry.
fn path_to_relative(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok().map(normalize_posix)
}

/// Convert a path to posix separators without touching the filesystem.
fn normalize_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Directories without any file underneath: collect every ancestor directory
/// of a file, then keep the visited directories that are not among them.
fn find_empty_dirs(dirs: &[String], files: &[FileState]) -> Vec<String> {
    let mut non_empty: HashSet<String> = HashSet::new();
    for file in files {
        let mut dir = PathBuf::from(&file.path);
        while let Some(parent) = dir.parent() {
            let s = normalize_posix(parent);
            if s.is_empty() || s == "." {
                break;
            }
            non_empty.insert(s);
            dir = parent.to_path_buf();
        }
    }
    dirs.iter()
        .filter(|dir| !non_empty.contains(*dir))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &[u8]) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn make_scanner(custom: Vec<&str>, behavior: FailureBehavior) -> WorkspaceScanner {
        WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: custom.into_iter().map(String::from).collect(),
            failure_behavior: behavior,
        })
    }

    #[test]
    fn scan_finds_files_and_hashes_them() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", b"hello a");
        write(dir.path(), "sub/b.txt", b"hello b");

        let scanner = make_scanner(vec![], FailureBehavior::Warn);
        let scan = scanner.scan(dir.path()).unwrap();

        let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["a.txt".to_string(), "sub/b.txt".to_string()]);
        assert_eq!(scan.files[0].hash, sha256_hex(b"hello a"));
        assert_eq!(scan.files[0].size, 7);
    }

    #[test]
    fn scan_skips_hardcoded_ignore_dirs_at_any_depth() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "keep.txt", b"keep");
        write(dir.path(), ".git/config", b"git");
        write(dir.path(), "node_modules/lib/x.js", b"js");
        write(dir.path(), "a/node_modules/y.js", b"y");

        let scanner = make_scanner(vec![], FailureBehavior::Warn);
        let scan = scanner.scan(dir.path()).unwrap();

        let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths, vec!["keep.txt".to_string()]);
        assert!(!scan.dirs.contains(&".git".to_string()));
        assert!(!scan.dirs.contains(&"node_modules".to_string()));
    }

    #[test]
    fn scan_applies_gitignore_with_directory_prefix() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "keep.txt", b"keep");
        write(dir.path(), ".gitignore", b"build/\n*.log\n");
        write(dir.path(), "build/out.o", b"obj");
        write(dir.path(), "a/build/x.o", b"x");
        write(dir.path(), "sub/debug.log", b"log");
        write(dir.path(), "sub/app.rs", b"rs");

        let scanner = make_scanner(vec![], FailureBehavior::Warn);
        let scan = scanner.scan(dir.path()).unwrap();

        let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![
                ".gitignore".to_string(),
                "keep.txt".to_string(),
                "sub/app.rs".to_string()
            ]
        );
    }

    #[test]
    fn scan_applies_nested_gitignore_with_prefix_join() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "top.txt", b"top");
        write(dir.path(), "src/.gitignore", b"gen/\n");
        write(dir.path(), "src/gen/g.rs", b"g");
        write(dir.path(), "src/keep.rs", b"keep");

        let scanner = make_scanner(vec![], FailureBehavior::Warn);
        let scan = scanner.scan(dir.path()).unwrap();

        let mut paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![
                "src/.gitignore".to_string(),
                "src/keep.rs".to_string(),
                "top.txt".to_string()
            ]
        );
    }

    #[test]
    fn scan_applies_custom_ignore_patterns() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", b"a");
        write(dir.path(), "tmp/x.tmp", b"x");

        let scanner = make_scanner(vec!["tmp/"], FailureBehavior::Warn);
        let scan = scanner.scan(dir.path()).unwrap();

        let paths: Vec<_> = scan.files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths, vec!["a.txt".to_string()]);
    }

    #[test]
    fn scan_detects_empty_dirs() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a/b/c.txt", b"c");
        write(dir.path(), "d/e/f.txt", b"f");
        std::fs::create_dir_all(dir.path().join("empty/nested")).unwrap();

        let scanner = make_scanner(vec![], FailureBehavior::Warn);
        let scan = scanner.scan(dir.path()).unwrap();

        let mut empty: Vec<_> = scan.empty_dirs.clone();
        empty.sort();
        assert_eq!(empty, vec!["empty".to_string(), "empty/nested".to_string()]);
        // Dir 'a' and 'a/b' contain files, so they are not empty.
        assert!(!scan.empty_dirs.contains(&"a".to_string()));
    }

    #[test]
    fn scan_ignore_behavior_skips_unreadable_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", b"a");
        // A dangling symlink cannot be read as a file.
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("broken.txt"))
            .unwrap();

        for behavior in [FailureBehavior::Warn, FailureBehavior::Ignore] {
            let scanner = make_scanner(vec![], behavior);
            let scan = scanner.scan(dir.path()).unwrap();
            assert_eq!(scan.files.len(), 1);
        }
    }

    #[test]
    fn is_ignored_matches_bare_names_at_any_depth() {
        let scanner = make_scanner(vec!["dist"], FailureBehavior::Warn);
        assert!(scanner.is_ignored("dist"));
        assert!(scanner.is_ignored("dist/out.txt"));
        assert!(scanner.is_ignored("a/b/dist"));
        assert!(scanner.is_ignored("a/b/dist/x.txt"));
        assert!(!scanner.is_ignored("a/dist-x/y.txt"));
    }

    #[test]
    fn hardcoded_ignored_protects_git_and_node_modules() {
        assert!(is_hardcoded_ignored(".git"));
        assert!(is_hardcoded_ignored(".git/config"));
        assert!(is_hardcoded_ignored("a/node_modules/x.js"));
        assert!(!is_hardcoded_ignored("src/main.rs"));
    }
}
