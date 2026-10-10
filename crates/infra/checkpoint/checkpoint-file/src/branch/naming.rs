//! Name predicates for feature branches (collaboration targets merged
//! as Git refs under `refs/wf/feat/`).

/// Reserved collaboration target names that would shadow the singleton
/// integration and external lines.
pub const RESERVED_FEATURE_NAMES: &[&str] = &["main", "human"];

/// Whether a feature name collides with a reserved singleton line.
pub fn is_reserved_feature_name(name: &str) -> bool {
    RESERVED_FEATURE_NAMES.contains(&name)
}

/// Feature branches are bare names: non-empty and without any slash.
pub fn is_feature_branch_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_names_reject_slash() {
        assert!(is_feature_branch_name("feature-1"));
        assert!(!is_feature_branch_name("other/abc"));
        assert!(!is_feature_branch_name(""));
    }

    #[test]
    fn reserved_names_cover_singletons() {
        assert!(is_reserved_feature_name("main"));
        assert!(is_reserved_feature_name("human"));
        assert!(!is_reserved_feature_name("feature-1"));
        assert!(!is_reserved_feature_name(""));
    }
}
