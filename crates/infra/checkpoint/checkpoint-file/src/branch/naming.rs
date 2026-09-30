/// Sole prefix identifying execution branches. Feature branches are bare
/// names without this prefix. The prefix check is intentionally strict so a
/// stray slash in a feature name is rejected instead of misclassified.
pub const EXECUTION_BRANCH_PREFIX: &str = "execution/";

pub fn execution_branch_name(entity_type: &str, entity_id: &str) -> String {
    format!("{entity_type}/{entity_id}")
}

/// Branch namespace distinguishing the two previously conflated models:
/// execution branches (`execution/{id}`, graph-blob history owned by the
/// `BranchStorageAdapter`) vs feature branches (`{feature}`, content-merge
/// pointers in layertwine's native `branches` table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    Execution,
    Feature,
}

/// Classify by explicit prefix: only names starting with the execution
/// prefix are execution branches, everything else is a feature branch.
pub fn classify_branch(name: &str) -> BranchKind {
    if is_execution_branch_name(name) {
        BranchKind::Execution
    } else {
        BranchKind::Feature
    }
}

/// Execution branches must carry the explicit prefix.
pub fn is_execution_branch_name(name: &str) -> bool {
    name.starts_with(EXECUTION_BRANCH_PREFIX)
}

/// Feature branches are bare names: non-empty and without any slash.
pub fn is_feature_branch_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/')
}

pub fn branch_entity_type(branch_name: &str) -> Option<&str> {
    let remainder = branch_name.strip_prefix(EXECUTION_BRANCH_PREFIX)?;
    if remainder.is_empty() {
        return None;
    }
    branch_name.split('/').next()
}

pub fn branch_entity_id(branch_name: &str) -> Option<&str> {
    let remainder = branch_name.strip_prefix(EXECUTION_BRANCH_PREFIX)?;
    if remainder.is_empty() {
        return None;
    }
    Some(remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_branch_name_formats() {
        assert_eq!(
            execution_branch_name("execution", "abc123"),
            "execution/abc123"
        );
    }

    #[test]
    fn parse_entity_type() {
        assert_eq!(branch_entity_type("execution/abc"), Some("execution"));
        assert_eq!(branch_entity_type("nonslash"), None);
    }

    #[test]
    fn parse_entity_id() {
        assert_eq!(branch_entity_id("execution/abc"), Some("abc"));
        assert_eq!(branch_entity_id("nonslash"), None);
        assert_eq!(branch_entity_id("other/abc"), None);
    }

    #[test]
    fn parse_entity_id_keeps_full_remainder() {
        assert_eq!(branch_entity_id("execution/a/b"), Some("a/b"));
        assert_eq!(branch_entity_id("execution/"), None);
        assert_eq!(branch_entity_id("execution"), None);
    }

    #[test]
    fn parse_entity_type_rejects_empty_remainder() {
        assert_eq!(branch_entity_type("execution/abc"), Some("execution"));
        assert_eq!(branch_entity_type("execution/"), None);
        assert_eq!(branch_entity_type("execution"), None);
    }

    #[test]
    fn classify_branch_by_namespace() {
        assert_eq!(classify_branch("execution/abc"), BranchKind::Execution);
        assert_eq!(classify_branch("feature-1"), BranchKind::Feature);
        assert_eq!(classify_branch("branch-1"), BranchKind::Feature);
        assert_eq!(classify_branch("other/abc"), BranchKind::Feature);
    }

    #[test]
    fn feature_names_reject_slash() {
        assert!(is_feature_branch_name("feature-1"));
        assert!(!is_feature_branch_name("other/abc"));
        assert!(!is_feature_branch_name("execution/abc"));
        assert!(!is_feature_branch_name(""));
        assert!(is_execution_branch_name("execution/abc"));
        assert!(!is_execution_branch_name("other/abc"));
    }
}
