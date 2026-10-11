//! Execution file-scope merge, validation, and expansion.
//!
//! Implements the reusable inheritance semantics shared by the global
//! default scope and future per-execution overrides: missing values inherit
//! the next lower layer, explicit values (including explicit empty lists,
//! which deny everything) override it. Checkpoint prefixes must stay inside
//! the write prefixes; a violation fails instead of silently narrowing.

use crate::error::{ConfigError, ConfigResult};
use wf_types::config::file_scope::{
    scope_path_contains, validate_scope_path, ExecutionFileScope, ResolvedFileScope,
};

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
/// first, call argument last). Folding with [`merge_file_scope`] keeps the
/// highest set value per field.
pub fn merge_file_scope_chain<'a>(
    layers: impl IntoIterator<Item = &'a ExecutionFileScope>,
) -> ExecutionFileScope {
    let mut merged = ExecutionFileScope::default();
    for layer in layers {
        merged = merge_file_scope(&merged, layer);
    }
    merged
}

/// Validate one lexical path entry, mapping the failure into a config error.
fn validate_entry(path: &str, field: &str) -> ConfigResult<()> {
    validate_scope_path(path, field).map_err(ConfigError::Validation)
}

/// Validate an optional prefix list entry by entry. Missing stays missing;
/// an explicit empty list is valid and means denial.
pub fn validate_optional_scope_path_list(
    entries: Option<&[String]>,
    field: &str,
) -> ConfigResult<()> {
    if let Some(list) = entries {
        validate_scope_path_list(list, field)?;
    }
    Ok(())
}

/// Validate a concrete prefix list entry by entry.
pub fn validate_scope_path_list(entries: &[String], field: &str) -> ConfigResult<()> {
    for entry in entries {
        validate_entry(entry, field)?;
    }
    Ok(())
}

/// Validate an optional single path (workdir, workspace root, database path
/// style values). Missing stays missing; blank values fail.
pub fn validate_optional_scope_path(value: Option<&str>, field: &str) -> ConfigResult<()> {
    if let Some(path) = value {
        validate_entry(path, field)?;
    }
    Ok(())
}

/// Validate an unexpanded scope: every listed prefix must be lexically
/// valid, checkpoint prefixes must already be inside the write prefixes
/// when both are concretely known, and a workdir must sit inside the read
/// or write prefixes when both are concretely known. Layers left for
/// inheritance (`None`) are resolved by [`expand_file_scope`] instead.
pub fn validate_file_scope(scope: &ExecutionFileScope) -> ConfigResult<()> {
    validate_optional_scope_path_list(scope.read_prefixes.as_deref(), "scope.read_prefixes")?;
    validate_optional_scope_path_list(scope.write_prefixes.as_deref(), "scope.write_prefixes")?;
    validate_optional_scope_path_list(
        scope.checkpoint_prefixes.as_deref(),
        "scope.checkpoint_prefixes",
    )?;
    validate_optional_scope_path(scope.workdir.as_deref(), "scope.workdir")?;
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

/// Expand a scope into concrete values: a missing checkpoint list copies the
/// resolved write list, missing read/write lists fall back to the global
/// default then to the workspace root, a missing flag falls back to the
/// global default then to enabled, and a missing workdir falls back to the
/// global default. Downstream consumers only see the expanded result.
pub fn expand_file_scope(
    scope: &ExecutionFileScope,
    global_default: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> ResolvedFileScope {
    let read_prefixes = resolve_prefix_list(
        scope.read_prefixes.as_ref(),
        global_default.and_then(|g| g.read_prefixes.as_ref()),
        workspace_root,
    );
    let write_prefixes = resolve_prefix_list(
        scope.write_prefixes.as_ref(),
        global_default.and_then(|g| g.write_prefixes.as_ref()),
        workspace_root,
    );
    let checkpoint_prefixes = scope
        .checkpoint_prefixes
        .clone()
        .or_else(|| global_default.and_then(|g| g.checkpoint_prefixes.clone()))
        .unwrap_or_else(|| write_prefixes.clone());
    let enabled = scope
        .enabled
        .or_else(|| global_default.and_then(|g| g.enabled))
        .unwrap_or(true);
    let workdir = scope
        .workdir
        .clone()
        .or_else(|| global_default.and_then(|g| g.workdir.clone()));
    ResolvedFileScope {
        enabled,
        read_prefixes,
        write_prefixes,
        checkpoint_prefixes,
        workdir,
    }
}

/// Validate a fully expanded scope: lexical checks plus unconditional
/// checkpoint-inside-write and workdir-inside-bounds enforcement, since no
/// layer is left to inherit.
pub fn validate_resolved_file_scope(scope: &ResolvedFileScope) -> ConfigResult<()> {
    validate_scope_path_list(&scope.read_prefixes, "scope.read_prefixes")?;
    validate_scope_path_list(&scope.write_prefixes, "scope.write_prefixes")?;
    validate_scope_path_list(&scope.checkpoint_prefixes, "scope.checkpoint_prefixes")?;
    if let Some(workdir) = scope.workdir.as_deref() {
        validate_entry(workdir, "scope.workdir")?;
    }
    check_checkpoint_subset(&scope.checkpoint_prefixes, &scope.write_prefixes)?;
    if let Some(workdir) = scope.workdir.as_deref() {
        check_workdir_within_bounds(workdir, &scope.read_prefixes, &scope.write_prefixes)?;
    }
    Ok(())
}

fn resolve_prefix_list(
    scope: Option<&Vec<String>>,
    global: Option<&Vec<String>>,
    workspace_root: Option<&str>,
) -> Vec<String> {
    if let Some(list) = scope {
        return list.clone();
    }
    if let Some(list) = global {
        return list.clone();
    }
    workspace_root
        .map(|root| vec![root.to_string()])
        .unwrap_or_default()
}

fn check_checkpoint_subset(checkpoint: &[String], write: &[String]) -> ConfigResult<()> {
    for prefix in checkpoint {
        if !write.iter().any(|bound| scope_path_contains(bound, prefix)) {
            return Err(ConfigError::Validation(format!(
                "scope.checkpoint_prefixes entry '{prefix}' is outside scope.write_prefixes"
            )));
        }
    }
    Ok(())
}

fn check_workdir_within_bounds(
    workdir: &str,
    read: &[String],
    write: &[String],
) -> ConfigResult<()> {
    let within_read = read.iter().any(|bound| scope_path_contains(bound, workdir));
    let within_write = write
        .iter()
        .any(|bound| scope_path_contains(bound, workdir));
    if !within_read && !within_write {
        return Err(ConfigError::Validation(format!(
            "scope.workdir '{workdir}' is outside scope.read_prefixes and scope.write_prefixes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::config::file_scope::is_scope_subset;

    fn scope(
        enabled: Option<bool>,
        read: Option<Vec<&str>>,
        write: Option<Vec<&str>>,
        checkpoint: Option<Vec<&str>>,
        workdir: Option<&str>,
    ) -> ExecutionFileScope {
        let owned = |list: Option<Vec<&str>>| {
            list.map(|items| items.into_iter().map(str::to_string).collect())
        };
        ExecutionFileScope {
            enabled,
            read_prefixes: owned(read),
            write_prefixes: owned(write),
            checkpoint_prefixes: owned(checkpoint),
            workdir: workdir.map(str::to_string),
        }
    }

    #[test]
    fn missing_values_inherit_while_set_values_override() {
        let base = scope(
            Some(true),
            Some(vec!["/repo"]),
            Some(vec!["/repo/app"]),
            None,
            None,
        );
        let upper = scope(None, None, Some(vec!["/repo/web"]), Some(vec![]), None);
        let merged = merge_file_scope(&base, &upper);
        assert_eq!(merged.enabled, Some(true));
        assert_eq!(merged.read_prefixes, Some(vec!["/repo".to_string()]));
        assert_eq!(merged.write_prefixes, Some(vec!["/repo/web".to_string()]));
        assert_eq!(merged.checkpoint_prefixes, Some(vec![]));
    }

    #[test]
    fn chain_merge_prefers_later_layers() {
        let global = scope(Some(true), None, Some(vec!["/repo"]), None, None);
        let parent = scope(None, None, Some(vec!["/repo/app"]), None, None);
        let call = scope(Some(false), None, None, None, None);
        let merged = merge_file_scope_chain([&global, &parent, &call]);
        assert_eq!(merged.enabled, Some(false));
        assert_eq!(merged.write_prefixes, Some(vec!["/repo/app".to_string()]));
    }

    #[test]
    fn explicit_empty_list_denies_and_stays_subset() {
        let denied = scope(None, Some(vec![]), Some(vec![]), Some(vec![]), None);
        assert!(validate_file_scope(&denied).is_ok());
        assert!(is_scope_subset(&[], &[String::from("/repo")]));
    }

    #[test]
    fn checkpoint_outside_write_fails_without_narrowing() {
        let bad = scope(
            None,
            None,
            Some(vec!["/repo/app"]),
            Some(vec!["/repo/other"]),
            None,
        );
        let err = validate_file_scope(&bad).expect_err("out-of-bounds checkpoint must fail");
        assert!(err.to_string().contains("/repo/other"), "error: {err}");
    }

    #[test]
    fn workdir_outside_bounds_fails() {
        let bad = scope(
            None,
            Some(vec!["/repo/app"]),
            Some(vec!["/repo/app"]),
            None,
            Some("/repo/other"),
        );
        let err = validate_file_scope(&bad).expect_err("out-of-bounds workdir must fail");
        assert!(err.to_string().contains("/repo/other"), "error: {err}");
    }

    #[test]
    fn expansion_copies_write_into_missing_checkpoint() {
        let partial = scope(None, None, Some(vec!["/repo/app"]), None, None);
        let expanded = expand_file_scope(&partial, None, Some("/repo"));
        assert!(expanded.enabled);
        assert_eq!(expanded.read_prefixes, vec!["/repo".to_string()]);
        assert_eq!(expanded.write_prefixes, vec!["/repo/app".to_string()]);
        assert_eq!(expanded.checkpoint_prefixes, vec!["/repo/app".to_string()]);
        assert!(validate_resolved_file_scope(&expanded).is_ok());
    }

    #[test]
    fn expansion_keeps_explicit_empty_checkpoint_as_denial() {
        let denied = scope(None, None, Some(vec!["/repo/app"]), Some(vec![]), None);
        let expanded = expand_file_scope(&denied, None, Some("/repo"));
        assert!(expanded.checkpoint_prefixes.is_empty());
        assert!(validate_resolved_file_scope(&expanded).is_ok());
    }

    #[test]
    fn expanded_out_of_bounds_checkpoint_fails() {
        let expanded = ResolvedFileScope {
            enabled: true,
            read_prefixes: vec!["/repo".to_string()],
            write_prefixes: vec!["/repo/app".to_string()],
            checkpoint_prefixes: vec!["/repo/other".to_string()],
            workdir: None,
        };
        assert!(validate_resolved_file_scope(&expanded).is_err());
    }
}
