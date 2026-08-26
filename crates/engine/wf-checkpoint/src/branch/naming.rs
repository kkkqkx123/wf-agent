pub fn execution_branch_name(entity_type: &str, entity_id: &str) -> String {
    format!("{}/{}", entity_type, entity_id)
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
}
