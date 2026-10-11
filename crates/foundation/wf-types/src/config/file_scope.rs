use serde::{Deserialize, Serialize};

/// Longest accepted scope path in characters. The limit is lexical only.
pub const MAX_SCOPE_PATH_LEN: usize = 1024;

/// Per-execution filesystem scope. Every field is optional: a missing value
/// inherits the enclosing layer, while an explicit empty list denies
/// everything. A non-empty list is the allowed set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecutionFileScope {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub read_prefixes: Option<Vec<String>>,
    #[serde(default)]
    pub write_prefixes: Option<Vec<String>>,
    #[serde(default)]
    pub checkpoint_prefixes: Option<Vec<String>>,
    #[serde(default)]
    pub workdir: Option<String>,
}

/// Fully resolved scope consumed downstream. Every prefix list is concrete:
/// an empty list denies everything, a non-empty list is the allowed set.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFileScope {
    pub enabled: bool,
    pub read_prefixes: Vec<String>,
    pub write_prefixes: Vec<String>,
    pub checkpoint_prefixes: Vec<String>,
    pub workdir: Option<String>,
}

/// Lexically normalize a path without touching the filesystem: collapse
/// duplicate separators and single-dot segments, resolve double-dot segments
/// against the preceding segment, and strip trailing separators.
pub fn normalize_scope_path(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    let mut out = parts.join("/");
    if absolute {
        out.insert(0, '/');
    }
    if out.is_empty() {
        out = if absolute {
            "/".to_string()
        } else {
            ".".to_string()
        };
    }
    out
}

/// Lexically validate a scope path: reject blank values, embedded NUL
/// bytes, overlong values, and relative forms that escape their base. No
/// filesystem access happens here; existence and link resolution belong to
/// the bootstrap and execution edges.
pub fn validate_scope_path(path: &str, field: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if path.contains('\0') {
        return Err(format!("{field} must not contain NUL"));
    }
    if path.len() > MAX_SCOPE_PATH_LEN {
        return Err(format!(
            "{field} must be at most {MAX_SCOPE_PATH_LEN} characters"
        ));
    }
    if !path.starts_with('/') && !is_relative_contained(path) {
        return Err(format!(
            "{field} must be absolute or relative to the workspace, got '{path}'"
        ));
    }
    Ok(())
}

/// True when a relative path never climbs above its base: scanning left to
/// right, every double-dot segment must consume a preceding real segment.
/// Normalizing first would hide the escape (`../x` collapses to `x`), so
/// the raw segments are measured here.
fn is_relative_contained(path: &str) -> bool {
    let mut depth = 0i32;
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => depth += 1,
        }
    }
    true
}

/// Directory-boundary-aware prefix containment on normalized forms: equal
/// paths contain each other, otherwise the candidate must extend the prefix
/// past a separator so sibling names sharing a string prefix do not match.
pub fn scope_path_contains(prefix: &str, path: &str) -> bool {
    let parent = normalize_scope_path(prefix);
    let candidate = normalize_scope_path(path);
    if parent == "/" {
        return candidate.starts_with('/');
    }
    candidate == parent || candidate.starts_with(&format!("{parent}/"))
}

/// True when every inner prefix is contained in some outer prefix. An empty
/// inner set is a subset of anything: denying everything never overreaches.
pub fn is_scope_subset(inner: &[String], outer: &[String]) -> bool {
    inner
        .iter()
        .all(|path| outer.iter().any(|bound| scope_path_contains(bound, path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_collapses_separators_and_dots() {
        assert_eq!(normalize_scope_path("/a//b/./c/"), "/a/b/c");
        assert_eq!(normalize_scope_path("/a/b/../c"), "/a/c");
        assert_eq!(normalize_scope_path("a/b"), "a/b");
        assert_eq!(normalize_scope_path("/"), "/");
    }

    #[test]
    fn validation_rejects_blank_nul_overlong_and_escaping() {
        assert!(validate_scope_path("", "p").is_err());
        assert!(validate_scope_path("   ", "p").is_err());
        assert!(validate_scope_path("a\0b", "p").is_err());
        assert!(validate_scope_path(&"a".repeat(2048), "p").is_err());
        assert!(validate_scope_path("../escape", "p").is_err());
        assert!(validate_scope_path("a/../../escape", "p").is_err());
        assert!(validate_scope_path("..", "p").is_err());
        assert!(validate_scope_path("/abs/path", "p").is_ok());
        assert!(validate_scope_path("workspace/relative", "p").is_ok());
        assert!(validate_scope_path("a/b/../c", "p").is_ok());
    }

    #[test]
    fn containment_respects_directory_boundaries() {
        assert!(scope_path_contains("/repo/app", "/repo/app"));
        assert!(scope_path_contains("/repo/app", "/repo/app/src/main.rs"));
        assert!(!scope_path_contains("/repo/app", "/repo/application"));
        assert!(!scope_path_contains("/repo/app", "/other/app"));
        assert!(scope_path_contains("/repo/app/", "/repo/app/src"));
        assert!(scope_path_contains("/", "/anything/at/all"));
        assert!(!scope_path_contains("/", "relative/path"));
    }

    #[test]
    fn subset_treats_empty_inner_as_contained() {
        let outer = vec!["/repo/app".to_string()];
        assert!(is_scope_subset(&[], &outer));
        assert!(is_scope_subset(&[String::from("/repo/app/src")], &outer));
        assert!(!is_scope_subset(&[String::from("/repo/other")], &outer));
    }
}
