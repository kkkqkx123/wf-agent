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

/// Merge two scope layers: every value set on `upper` wins (an explicit
/// empty list wins as a denial), every missing value inherits `base`.
pub fn merge_file_scope(
    base: &ExecutionFileScope,
    upper: &ExecutionFileScope,
) -> ExecutionFileScope {
    ExecutionFileScope {
        enabled: upper.enabled.or(base.enabled),
        read_prefixes: upper
            .read_prefixes
            .clone()
            .or_else(|| base.read_prefixes.clone()),
        write_prefixes: upper
            .write_prefixes
            .clone()
            .or_else(|| base.write_prefixes.clone()),
        checkpoint_prefixes: upper
            .checkpoint_prefixes
            .clone()
            .or_else(|| base.checkpoint_prefixes.clone()),
        workdir: upper.workdir.clone().or_else(|| base.workdir.clone()),
    }
}

/// Merge a layer chain given from lowest to highest priority (global default
/// first, call argument last). Folding keeps the highest set value per field.
pub fn merge_file_scope_chain<'a>(
    layers: impl IntoIterator<Item = &'a ExecutionFileScope>,
) -> ExecutionFileScope {
    let mut merged = ExecutionFileScope::default();
    for layer in layers {
        merged = merge_file_scope(&merged, layer);
    }
    merged
}

fn validate_entry(path: &str, field: &str) -> Result<(), String> {
    validate_scope_path(path, field)
}

fn validate_path_list(entries: &[String], field: &str) -> Result<(), String> {
    for entry in entries {
        validate_entry(entry, field)?;
    }
    Ok(())
}

fn validate_optional_path_list(entries: Option<&[String]>, field: &str) -> Result<(), String> {
    if let Some(list) = entries {
        validate_path_list(list, field)?;
    }
    Ok(())
}

fn validate_optional_path(value: Option<&str>, field: &str) -> Result<(), String> {
    if let Some(path) = value {
        validate_entry(path, field)?;
    }
    Ok(())
}

fn check_checkpoint_subset(checkpoint: &[String], write: &[String]) -> Result<(), String> {
    for prefix in checkpoint {
        if !write.iter().any(|bound| scope_path_contains(bound, prefix)) {
            return Err(format!(
                "scope.checkpoint_prefixes entry '{prefix}' is outside scope.write_prefixes"
            ));
        }
    }
    Ok(())
}

fn check_workdir_within_bounds(
    workdir: &str,
    read: &[String],
    write: &[String],
) -> Result<(), String> {
    let within_read = read.iter().any(|bound| scope_path_contains(bound, workdir));
    let within_write = write
        .iter()
        .any(|bound| scope_path_contains(bound, workdir));
    if !within_read && !within_write {
        return Err(format!(
            "scope.workdir '{workdir}' is outside scope.read_prefixes and scope.write_prefixes"
        ));
    }
    Ok(())
}

/// Validate an unexpanded scope: every listed prefix must be lexically
/// valid, checkpoint prefixes must already be inside the write prefixes
/// when both are concretely known, and a workdir must sit inside the read
/// or write prefixes when both are concretely known.
pub fn validate_file_scope(scope: &ExecutionFileScope) -> Result<(), String> {
    validate_optional_path_list(scope.read_prefixes.as_deref(), "scope.read_prefixes")?;
    validate_optional_path_list(scope.write_prefixes.as_deref(), "scope.write_prefixes")?;
    validate_optional_path_list(
        scope.checkpoint_prefixes.as_deref(),
        "scope.checkpoint_prefixes",
    )?;
    validate_optional_path(scope.workdir.as_deref(), "scope.workdir")?;
    if let (Some(checkpoint), Some(write)) = (
        scope.checkpoint_prefixes.as_deref(),
        scope.write_prefixes.as_deref(),
    ) {
        check_checkpoint_subset(checkpoint, write)?;
    }
    if let (Some(read), Some(write), Some(workdir)) = (
        scope.read_prefixes.as_deref(),
        scope.write_prefixes.as_deref(),
        scope.workdir.as_deref(),
    ) {
        check_workdir_within_bounds(workdir, read, write)?;
    }
    Ok(())
}

fn absolutize_prefix(prefix: &str, workspace_root: Option<&str>) -> String {
    if prefix.starts_with('/') {
        return normalize_scope_path(prefix);
    }
    if let Some(root) = workspace_root {
        if root.starts_with('/') && !root.trim().is_empty() {
            let joined = format!("{}/{}", normalize_scope_path(root), prefix);
            return normalize_scope_path(&joined);
        }
    }
    normalize_scope_path(prefix)
}

fn resolve_prefix_list(scope: Option<&Vec<String>>, global: Option<&Vec<String>>) -> Vec<String> {
    if let Some(list) = scope {
        return list.clone();
    }
    if let Some(list) = global {
        return list.clone();
    }
    Vec::new()
}

/// Expand a scope into concrete values: a missing checkpoint list copies the
/// resolved write list, missing read/write lists fall back to the global
/// default and otherwise deny everything, a missing flag falls back to the
/// global default then to enabled, and a missing workdir falls back to the
/// global default. Relative entries are joined against the workspace root
/// when one is known so enforcement sees absolute forms.
pub fn expand_file_scope(
    scope: &ExecutionFileScope,
    global_default: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> ResolvedFileScope {
    let read_prefixes: Vec<String> = resolve_prefix_list(
        scope.read_prefixes.as_ref(),
        global_default.and_then(|g| g.read_prefixes.as_ref()),
    )
    .iter()
    .map(|p| absolutize_prefix(p, workspace_root))
    .collect();
    let write_prefixes: Vec<String> = resolve_prefix_list(
        scope.write_prefixes.as_ref(),
        global_default.and_then(|g| g.write_prefixes.as_ref()),
    )
    .iter()
    .map(|p| absolutize_prefix(p, workspace_root))
    .collect();
    let checkpoint_prefixes: Vec<String> = scope
        .checkpoint_prefixes
        .clone()
        .or_else(|| global_default.and_then(|g| g.checkpoint_prefixes.clone()))
        .unwrap_or_else(|| write_prefixes.clone())
        .iter()
        .map(|p| absolutize_prefix(p, workspace_root))
        .collect();
    let enabled = scope
        .enabled
        .or_else(|| global_default.and_then(|g| g.enabled))
        .unwrap_or(true);
    let workdir = scope
        .workdir
        .clone()
        .or_else(|| global_default.and_then(|g| g.workdir.clone()))
        .map(|w| absolutize_prefix(&w, workspace_root));
    ResolvedFileScope {
        enabled,
        read_prefixes,
        write_prefixes,
        checkpoint_prefixes,
        workdir,
    }
}

fn check_within_workspace(prefixes: &[String], workspace_root: &str) -> Result<(), String> {
    let root = normalize_scope_path(workspace_root);
    if root == "." {
        for prefix in prefixes {
            if prefix.starts_with('/') {
                return Err(format!(
                    "scope entry '{prefix}' is outside workspace root '{root}'"
                ));
            }
        }
        return Ok(());
    }
    for prefix in prefixes {
        if !scope_path_contains(&root, prefix) {
            return Err(format!(
                "scope entry '{prefix}' is outside workspace root '{root}'"
            ));
        }
    }
    Ok(())
}

/// Validate a fully expanded scope: lexical checks plus unconditional
/// checkpoint-inside-write and workdir-inside-bounds enforcement.
pub fn validate_resolved_file_scope(scope: &ResolvedFileScope) -> Result<(), String> {
    validate_path_list(&scope.read_prefixes, "scope.read_prefixes")?;
    validate_path_list(&scope.write_prefixes, "scope.write_prefixes")?;
    validate_path_list(&scope.checkpoint_prefixes, "scope.checkpoint_prefixes")?;
    if let Some(workdir) = scope.workdir.as_deref() {
        validate_entry(workdir, "scope.workdir")?;
    }
    check_checkpoint_subset(&scope.checkpoint_prefixes, &scope.write_prefixes)?;
    if let Some(workdir) = scope.workdir.as_deref() {
        check_workdir_within_bounds(workdir, &scope.read_prefixes, &scope.write_prefixes)?;
    }
    Ok(())
}

/// Check that a child scope stays inside its parent. An empty child set is
/// always contained; any out-of-bounds entry reports the offending path.
pub fn check_child_scope_subset(
    child: &ResolvedFileScope,
    parent: &ResolvedFileScope,
) -> Result<(), String> {
    for prefix in child
        .read_prefixes
        .iter()
        .chain(child.write_prefixes.iter())
        .chain(child.checkpoint_prefixes.iter())
    {
        let inside_read = parent
            .read_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix));
        let inside_write = parent
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix));
        if !inside_read && !inside_write {
            return Err(format!(
                "child scope entry '{prefix}' is outside parent read and write prefixes"
            ));
        }
    }
    for prefix in &child.checkpoint_prefixes {
        if !child
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix))
        {
            return Err(format!(
                "checkpoint prefix '{prefix}' is outside child write prefixes"
            ));
        }
    }
    if let Some(workdir) = child.workdir.as_deref() {
        let inside_read = child
            .read_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, workdir));
        let inside_write = child
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, workdir));
        if !inside_read && !inside_write {
            return Err(format!(
                "workdir '{workdir}' is outside child read and write prefixes"
            ));
        }
    }
    Ok(())
}

/// True when the resolved scope disables the file channel. Closed
/// executions create no session, run no scan, and record no file link.
pub fn is_file_channel_closed(scope: Option<&ResolvedFileScope>) -> bool {
    scope.is_some_and(|s| !s.enabled)
}

/// Resolve the effective execution scope from the layered inputs.
/// Priority from low to high is global default, parent execution, node
/// declaration, then call argument.
pub fn resolve_execution_scope(
    global_default: Option<&ExecutionFileScope>,
    parent: Option<&ExecutionFileScope>,
    node: Option<&ExecutionFileScope>,
    call: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> ResolvedFileScope {
    let merged = merge_file_scope_chain([global_default, parent, node, call].into_iter().flatten());
    // Re-expand against the global default so missing checkpoint lists copy
    // the merged write list while missing read/write lists still fall back
    // to the global default and otherwise deny everything.
    let merged_with_global =
        merge_file_scope(&global_default.cloned().unwrap_or_default(), &merged);
    expand_file_scope(&merged_with_global, global_default, workspace_root)
}

/// Resolve and validate the effective scope. Lexical failures, checkpoint
/// outside write, workdir outside bounds, and child outside parent all
/// fail instead of silently narrowing.
pub fn resolve_execution_scope_checked(
    global_default: Option<&ExecutionFileScope>,
    parent_resolved: Option<&ResolvedFileScope>,
    node: Option<&ExecutionFileScope>,
    call: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> Result<ResolvedFileScope, String> {
    let parent_exec = parent_resolved.map(|p| ExecutionFileScope {
        enabled: Some(p.enabled),
        read_prefixes: Some(p.read_prefixes.clone()),
        write_prefixes: Some(p.write_prefixes.clone()),
        checkpoint_prefixes: Some(p.checkpoint_prefixes.clone()),
        workdir: p.workdir.clone(),
    });
    let resolved = resolve_execution_scope(
        global_default,
        parent_exec.as_ref(),
        node,
        call,
        workspace_root,
    );
    for path in resolved
        .read_prefixes
        .iter()
        .chain(resolved.write_prefixes.iter())
        .chain(resolved.checkpoint_prefixes.iter())
    {
        validate_scope_path(path, "scope.prefix")?;
    }
    if let Some(workdir) = resolved.workdir.as_deref() {
        validate_scope_path(workdir, "scope.workdir")?;
    }
    for prefix in &resolved.checkpoint_prefixes {
        if !resolved
            .write_prefixes
            .iter()
            .any(|bound| scope_path_contains(bound, prefix))
        {
            return Err(format!(
                "checkpoint prefix '{prefix}' is outside write prefixes"
            ));
        }
    }
    if let Some(workdir) = resolved.workdir.as_deref() {
        let inside = resolved
            .read_prefixes
            .iter()
            .chain(resolved.write_prefixes.iter())
            .any(|bound| scope_path_contains(bound, workdir));
        if !inside {
            return Err(format!(
                "workdir '{workdir}' is outside read and write prefixes"
            ));
        }
    }
    if let Some(root) = workspace_root {
        let all: Vec<String> = resolved
            .read_prefixes
            .iter()
            .chain(resolved.write_prefixes.iter())
            .chain(resolved.checkpoint_prefixes.iter())
            .cloned()
            .collect();
        check_within_workspace(&all, root)?;
        if let Some(workdir) = resolved.workdir.as_deref() {
            check_within_workspace(std::slice::from_ref(&workdir.to_string()), root)?;
        }
    }
    if let Some(parent) = parent_resolved {
        check_child_scope_subset(&resolved, parent)?;
    }
    Ok(resolved)
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

    #[test]
    fn relative_root_still_enforces_containment() {
        let inside = ExecutionFileScope {
            enabled: Some(true),
            read_prefixes: Some(vec!["work/sub".to_string()]),
            write_prefixes: Some(vec!["work/sub".to_string()]),
            checkpoint_prefixes: Some(vec!["work/sub".to_string()]),
            workdir: None,
        };
        assert!(
            resolve_execution_scope_checked(None, None, Some(&inside), None, Some("work")).is_ok()
        );
        let outside = ExecutionFileScope {
            enabled: Some(true),
            read_prefixes: Some(vec!["/etc".to_string()]),
            write_prefixes: Some(vec!["/etc".to_string()]),
            checkpoint_prefixes: Some(vec!["/etc".to_string()]),
            workdir: None,
        };
        assert!(
            resolve_execution_scope_checked(None, None, Some(&outside), None, Some("work"))
                .is_err()
        );
        let absolute_under_dot = ExecutionFileScope {
            enabled: Some(true),
            read_prefixes: Some(vec!["/etc".to_string()]),
            write_prefixes: Some(vec!["/etc".to_string()]),
            checkpoint_prefixes: Some(vec!["/etc".to_string()]),
            workdir: None,
        };
        assert!(resolve_execution_scope_checked(
            None,
            None,
            Some(&absolute_under_dot),
            None,
            Some(".")
        )
        .is_err());
    }
}
