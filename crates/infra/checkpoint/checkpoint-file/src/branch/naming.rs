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

/// Classify by naming convention: names containing `/` are execution
/// branches, bare names are feature/content branches.
pub fn classify_branch(name: &str) -> BranchKind {
    if name.contains('/') {
        BranchKind::Execution
    } else {
        BranchKind::Feature
    }
}

pub fn branch_entity_type(branch_name: &str) -> Option<&str> {
    branch_name.split('/').next()
}

pub fn branch_entity_id(branch_name: &str) -> Option<&str> {
    branch_name.split('/').nth(1)
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
        assert_eq!(branch_entity_type("nonslash"), Some("nonslash"));
    }

    #[test]
    fn parse_entity_id() {
        assert_eq!(branch_entity_id("execution/abc"), Some("abc"));
        assert_eq!(branch_entity_id("nonslash"), None);
    }

    #[test]
    fn classify_branch_by_namespace() {
        assert_eq!(classify_branch("execution/abc"), BranchKind::Execution);
        assert_eq!(classify_branch("feature-1"), BranchKind::Feature);
        assert_eq!(classify_branch("branch-1"), BranchKind::Feature);
    }
}
