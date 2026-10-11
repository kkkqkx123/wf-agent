//! Execution file-scope merge, validation, and expansion.
//!
//! Thin config-layer wrappers over the canonical scope helpers in
//! `wf_types::config::file_scope`. Missing values inherit the next lower
//! layer, explicit values (including explicit empty lists, which deny
//! everything) override it.

use crate::error::{ConfigError, ConfigResult};
use wf_types::config::file_scope::{ExecutionFileScope, ResolvedFileScope};

pub fn merge_file_scope(
    base: &ExecutionFileScope,
    upper: &ExecutionFileScope,
) -> ExecutionFileScope {
    wf_types::config::file_scope::merge_file_scope(base, upper)
}

pub fn merge_file_scope_chain<'a>(
    layers: impl IntoIterator<Item = &'a ExecutionFileScope>,
) -> ExecutionFileScope {
    wf_types::config::file_scope::merge_file_scope_chain(layers)
}

fn map_err(message: String) -> ConfigError {
    ConfigError::Validation(message)
}

pub fn validate_optional_scope_path_list(
    entries: Option<&[String]>,
    field: &str,
) -> ConfigResult<()> {
    for entry in entries.unwrap_or_default() {
        wf_types::config::file_scope::validate_scope_path(entry, field).map_err(map_err)?;
    }
    Ok(())
}

pub fn validate_scope_path_list(entries: &[String], field: &str) -> ConfigResult<()> {
    validate_optional_scope_path_list(Some(entries), field)
}

pub fn validate_optional_scope_path(value: Option<&str>, field: &str) -> ConfigResult<()> {
    if let Some(path) = value {
        wf_types::config::file_scope::validate_scope_path(path, field).map_err(map_err)?;
    }
    Ok(())
}

pub fn validate_file_scope(scope: &ExecutionFileScope) -> ConfigResult<()> {
    wf_types::config::file_scope::validate_file_scope(scope).map_err(map_err)
}

pub fn expand_file_scope(
    scope: &ExecutionFileScope,
    global_default: Option<&ExecutionFileScope>,
    workspace_root: Option<&str>,
) -> ResolvedFileScope {
    wf_types::config::file_scope::expand_file_scope(scope, global_default, workspace_root)
}

pub fn validate_resolved_file_scope(scope: &ResolvedFileScope) -> ConfigResult<()> {
    wf_types::config::file_scope::validate_resolved_file_scope(scope).map_err(map_err)
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
        assert!(expanded.read_prefixes.is_empty());
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
    fn expansion_denies_missing_lists_without_global() {
        let empty = scope(None, None, None, None, None);
        let expanded = expand_file_scope(&empty, None, Some("/repo"));
        assert!(expanded.read_prefixes.is_empty());
        assert!(expanded.write_prefixes.is_empty());
        assert!(expanded.checkpoint_prefixes.is_empty());
    }

    #[test]
    fn relative_entries_join_workspace_root() {
        let partial = scope(
            None,
            Some(vec!["sub"]),
            Some(vec!["sub/write"]),
            None,
            Some("sub/write"),
        );
        let expanded = expand_file_scope(&partial, None, Some("/repo"));
        assert_eq!(expanded.read_prefixes, vec!["/repo/sub".to_string()]);
        assert_eq!(expanded.write_prefixes, vec!["/repo/sub/write".to_string()]);
        assert_eq!(
            expanded.checkpoint_prefixes,
            vec!["/repo/sub/write".to_string()]
        );
        assert_eq!(expanded.workdir, Some("/repo/sub/write".to_string()));
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
