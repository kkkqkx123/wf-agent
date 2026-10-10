//! Name predicates for the two disjoint branch namespaces: execution
//! pointers (`execution-pointer/<id>`, execution index in `meta_kv`) and
//! bare feature names (collaboration targets merged as Git refs under
//! `refs/wf/feat/`).

/// Sole prefix identifying execution pointers. Feature branches are bare
/// names without this prefix. The prefix check is intentionally strict so a
/// stray slash in a feature name is rejected instead of misclassified.
pub const EXECUTION_POINTER_PREFIX: &str = "execution-pointer/";

pub fn execution_pointer_name(entity_id: &str) -> String {
    format!("{EXECUTION_POINTER_PREFIX}{entity_id}")
}

/// Execution pointers must carry the explicit prefix.
pub fn is_execution_pointer_name(name: &str) -> bool {
    match name.strip_prefix(EXECUTION_POINTER_PREFIX) {
        Some(remainder) => !remainder.is_empty(),
        None => false,
    }
}

/// Feature branches are bare names: non-empty and without any slash.
pub fn is_feature_branch_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_pointer_name_formats() {
        assert_eq!(execution_pointer_name("abc123"), "execution-pointer/abc123");
    }

    #[test]
    fn execution_names_require_non_empty_remainder() {
        assert!(is_execution_pointer_name("execution-pointer/abc"));
        assert!(is_execution_pointer_name("execution-pointer/a/b"));
        assert!(!is_execution_pointer_name("execution-pointer/"));
        assert!(!is_execution_pointer_name("execution-pointer"));
        assert!(!is_execution_pointer_name("nonslash"));
        assert!(!is_execution_pointer_name("other/abc"));
    }

    #[test]
    fn feature_names_reject_slash() {
        assert!(is_feature_branch_name("feature-1"));
        assert!(!is_feature_branch_name("other/abc"));
        assert!(!is_feature_branch_name("execution-pointer/abc"));
        assert!(!is_feature_branch_name(""));
        assert!(is_execution_pointer_name("execution-pointer/abc"));
        assert!(!is_execution_pointer_name("other/abc"));
        assert!(!is_execution_pointer_name("execution-pointer/"));
        assert!(!is_execution_pointer_name("execution-pointer"));
        assert!(!is_execution_pointer_name(""));
    }

    #[test]
    fn namespaces_are_disjoint() {
        for name in [
            "execution-pointer/abc",
            "execution-pointer/a/b",
            "feature-1",
            "other/abc",
            "execution-pointer",
            "execution-pointer/",
            "",
        ] {
            assert!(!(is_execution_pointer_name(name) && is_feature_branch_name(name)));
        }
    }
}
