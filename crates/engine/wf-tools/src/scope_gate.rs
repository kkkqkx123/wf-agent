//! Shared execution-scope path gate.
//!
//! Lexical prefix containment plus symlink-escape detection in one place,
//! consumed by the filesystem, shell and MCP executors so the three
//! implementations cannot drift apart. Policy (fail-closed writes, lenient
//! reads, error wording) stays with each caller; this module only decides
//! whether a resolved path is inside the allowed prefixes and, when the
//! filesystem could resolve it, returns the physical path for checkpoint
//! write gating.

use std::path::{Path, PathBuf};

use wf_types::config::file_scope::{normalize_scope_path, scope_path_contains, ResolvedFileScope};

/// Outcome of a path-in-bounds check. `Outside` carries the normalized path
/// when the lexical containment check failed; `Inside` carries the physical
/// path when canonicalization succeeded (or the lexical path when the
/// filesystem could not resolve it, matching the legacy behavior for
/// not-yet-existing files); `Escaped` carries the physical path that left
/// the prefixes through a symlink.
#[derive(Debug)]
pub enum PathVerdict {
    Outside(PathBuf),
    Inside(PathBuf),
    Escaped(PathBuf),
}

/// Short model-facing guidance listing the concrete prefixes in force.
/// Callers use this in scope violations so the model does not have to guess
/// which directories are reachable.
pub fn scope_guidance(scope: &ResolvedFileScope, kind: &str) -> String {
    let mut prefixes: Vec<String> = if kind == "write" {
        scope
            .write_prefixes
            .iter()
            .chain(scope.untracked_write_prefixes.iter())
            .cloned()
            .collect()
    } else {
        scope.read_prefixes.clone()
    };
    prefixes.sort();
    prefixes.dedup();
    let workdir = scope
        .workdir
        .as_deref()
        .map(|dir| format!(" Work inside '{dir}'."))
        .unwrap_or_default();
    let untracked = if kind == "write" && !scope.untracked_write_prefixes.is_empty() {
        " Untracked write prefixes are not restorable."
    } else {
        ""
    };
    format!(
        "{workdir} Allowed {kind} prefixes: {}.{untracked}",
        if prefixes.is_empty() {
            "none".to_string()
        } else {
            prefixes.join(", ")
        }
    )
}

/// Check `resolved` against `bounds`: lexical containment first, then the
/// physical path via direct canonicalize, falling back to the parent's
/// canonicalize joined with the file name (for files that do not exist yet
/// but sit under a symlinked directory).
pub fn check_path_bounds(resolved: &Path, bounds: &[String]) -> PathVerdict {
    let lexical = normalize_scope_path(&resolved.to_string_lossy());
    let lexically_inside = bounds
        .iter()
        .any(|bound| scope_path_contains(bound, &lexical));
    if !lexically_inside {
        return PathVerdict::Outside(PathBuf::from(&lexical));
    }
    let physical = resolved.canonicalize().ok().or_else(|| {
        resolved
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .and_then(|parent| parent.canonicalize().ok())
            .and_then(|pp| resolved.file_name().map(|name| pp.join(name)))
    });
    if let Some(physical) = physical {
        let physical_str = physical.to_string_lossy().to_string();
        if bounds
            .iter()
            .any(|bound| scope_path_contains(bound, &physical_str))
        {
            return PathVerdict::Inside(physical);
        }
        return PathVerdict::Escaped(physical);
    }
    PathVerdict::Inside(PathBuf::from(&lexical))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn relative_path_without_base_is_outside() {
        let verdict = check_path_bounds(Path::new("relative/file.txt"), &bounds(&["/repo"]));
        assert!(matches!(verdict, PathVerdict::Outside(_)));
    }

    #[test]
    fn missing_file_inside_prefix_is_inside() {
        let verdict = check_path_bounds(Path::new("/repo/does/not/exist.txt"), &bounds(&["/repo"]));
        assert!(matches!(verdict, PathVerdict::Inside(_)));
    }

    #[test]
    fn path_outside_prefixes_is_outside() {
        let verdict = check_path_bounds(Path::new("/etc/passwd"), &bounds(&["/repo"]));
        match verdict {
            PathVerdict::Outside(path) => {
                assert!(path.starts_with("/etc"));
            }
            other => panic!("expected outside rejection, got {other:?}"),
        }
    }
}
