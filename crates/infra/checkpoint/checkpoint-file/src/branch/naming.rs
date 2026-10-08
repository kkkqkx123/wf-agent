/// Sole prefix identifying execution branches. Feature branches are bare
/// names without this prefix. The prefix check is intentionally strict so a
/// stray slash in a feature name is rejected instead of misclassified.
pub const EXECUTION_BRANCH_PREFIX: &str = "execution/";

pub fn execution_branch_name(entity_id: &str) -> String {
    format!("{EXECUTION_BRANCH_PREFIX}{entity_id}")
}

/// Execution branches must carry the explicit prefix.
pub fn is_execution_branch_name(name: &str) -> bool {
    match name.strip_prefix(EXECUTION_BRANCH_PREFIX) {
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
    fn execution_branch_name_formats() {
        assert_eq!(execution_branch_name("abc123"), "execution/abc123");
    }

    #[test]
    fn execution_names_require_non_empty_remainder() {
        assert!(is_execution_branch_name("execution/abc"));
        assert!(is_execution_branch_name("execution/a/b"));
        assert!(!is_execution_branch_name("execution/"));
        assert!(!is_execution_branch_name("execution"));
        assert!(!is_execution_branch_name("nonslash"));
        assert!(!is_execution_branch_name("other/abc"));
    }

    #[test]
    fn feature_names_reject_slash() {
        assert!(is_feature_branch_name("feature-1"));
        assert!(!is_feature_branch_name("other/abc"));
        assert!(!is_feature_branch_name("execution/abc"));
        assert!(!is_feature_branch_name(""));
        assert!(is_execution_branch_name("execution/abc"));
        assert!(!is_execution_branch_name("other/abc"));
        assert!(!is_execution_branch_name("execution/"));
        assert!(!is_execution_branch_name("execution"));
        assert!(!is_execution_branch_name(""));
    }

    #[test]
    fn namespaces_are_disjoint() {
        for name in [
            "execution/abc",
            "execution/a/b",
            "feature-1",
            "other/abc",
            "execution",
            "execution/",
            "",
        ] {
            assert!(!(is_execution_branch_name(name) && is_feature_branch_name(name)));
        }
    }
}
