//! Skill file system abstraction.
//!
//! Separates the skill loader's file I/O from the concrete filesystem so
//! tests can inject faults (missing files, read failures) and future storage
//! backends (in-memory VFS, sandboxed access) can be plugged in without
//! touching the loader's business logic.

use std::io;
use std::path::{Path, PathBuf};

/// A single entry returned by [`SkillFileLoader::read_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDirEntry {
    /// Entry name (not the full path).
    pub name: String,
    /// Whether the entry is a directory (files are `false`).
    pub is_dir: bool,
}

/// Abstraction over the file operations used by the skill loader.
///
/// Synchronous by design: the skill loader call chain is fully synchronous
/// and does not depend on an async runtime. Implementations must be
/// `Send + Sync` because the loader is shared behind an `Arc`.
pub trait SkillFileLoader: Send + Sync {
    /// List the entries of a directory. The caller decides how to recurse.
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<SkillDirEntry>>;

    /// Read a UTF-8 text file.
    fn read_text(&self, path: &Path) -> io::Result<String>;

    /// Read a binary file.
    fn read_binary(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Whether the path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Canonicalize a path (used for traversal guards).
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
}

/// Default filesystem-backed implementation, wrapping `std::fs`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostSkillLoader;

impl SkillFileLoader for HostSkillLoader {
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<SkillDirEntry>> {
        std::fs::read_dir(dir)?
            .map(|entry| {
                let entry = entry?;
                Ok(SkillDirEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    is_dir: entry.file_type()?.is_dir(),
                })
            })
            .collect()
    }

    fn read_text(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn read_binary(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// In-memory `SkillFileLoader` for tests. Files live in a `HashMap` and
/// directories are inferred from path prefixes; `fail_read` injects read
/// failures for specific paths (missing files, permission errors).
#[cfg(test)]
#[derive(Default)]
pub struct InMemorySkillLoader {
    files: std::collections::HashMap<PathBuf, Vec<u8>>,
    fail_reads: std::collections::HashSet<PathBuf>,
}

#[cfg(test)]
impl InMemorySkillLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file; parent directories are created implicitly.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), content.into());
    }

    /// Make reads of `path` fail with a synthetic IO error.
    pub fn fail_read(&mut self, path: impl Into<PathBuf>) {
        self.fail_reads.insert(path.into());
    }

    fn has_children(&self, dir: &Path) -> bool {
        self.files.keys().any(|p| p.starts_with(dir))
    }
}

#[cfg(test)]
impl SkillFileLoader for InMemorySkillLoader {
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<SkillDirEntry>> {
        if !self.has_children(dir) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such directory"));
        }
        let mut entries: Vec<SkillDirEntry> = Vec::new();
        for path in self.files.keys() {
            let Ok(rel) = path.strip_prefix(dir) else {
                continue;
            };
            let mut components = rel.components();
            let Some(first) = components.next() else {
                continue;
            };
            let name = first.as_os_str().to_string_lossy().into_owned();
            let is_dir = components.next().is_some();
            if !entries.iter().any(|e| e.name == name) {
                entries.push(SkillDirEntry { name, is_dir });
            }
        }
        Ok(entries)
    }

    fn read_text(&self, path: &Path) -> io::Result<String> {
        if self.fail_reads.contains(path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected read failure",
            ));
        }
        let bytes = self.files.get(path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no such file: {}", path.display()),
            )
        })?;
        String::from_utf8(bytes.clone()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_binary(&self, path: &Path) -> io::Result<Vec<u8>> {
        if self.fail_reads.contains(path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected read failure",
            ));
        }
        self.files.get(path).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no such file: {}", path.display()),
            )
        })
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.has_children(path)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        if self.exists(path) {
            Ok(path.to_path_buf())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no such path: {}", path.display()),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree(root: &Path) {
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.md"), "a").unwrap();
        std::fs::write(root.join("sub/b.md"), "b").unwrap();
    }

    #[test]
    fn host_read_dir_reports_names_and_kinds() {
        let root = std::env::temp_dir().join(format!("wf-skillfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_tree(&root);

        let loader = HostSkillLoader;
        let entries = loader.read_dir(&root).unwrap();
        assert!(entries.iter().any(|e| e.name == "a.md" && !e.is_dir));
        assert!(entries.iter().any(|e| e.name == "sub" && e.is_dir));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn host_io_ops() {
        let root = std::env::temp_dir().join(format!("wf-skillfs2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_tree(&root);

        let loader = HostSkillLoader;
        let path = root.join("a.md");
        assert!(loader.exists(&path));
        assert!(!loader.exists(&root.join("nope.md")));
        assert_eq!(loader.read_text(&path).unwrap(), "a");
        assert_eq!(loader.read_binary(&path).unwrap(), b"a".to_vec());
        assert_eq!(
            loader.canonicalize(&path).unwrap(),
            std::fs::canonicalize(&path).unwrap()
        );
        assert!(loader.read_text(&root.join("missing.md")).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
