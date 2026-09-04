//! Contribution validation.
//!
//! Contributions are validated before registration: the contribution type
//! must be recognized and the registration key must be a non-empty string.
//! Validation is chained with the conflict-policy checks performed by the
//! registries.

/// The recognized contribution types (kebab-case identifiers): the 6
/// behavioral types plus the declarative resource contributions introduced
/// for plugin/resource integration.
pub const VALID_CONTRIBUTION_TYPES: &[&str] = &[
    "node-type",
    "tool-type",
    "llm-provider",
    "formatter",
    "event-handler",
    "middleware",
    // Declarative resource contributions
    "workflow",
    "prompt",
    "fragment",
    "agent-template",
    "node-template",
    "trigger",
    "tool-description",
    "tool",
];

/// Check that a contribution type is recognized.
pub fn is_valid_contribution_type(contribution_type: &str) -> bool {
    VALID_CONTRIBUTION_TYPES.contains(&contribution_type)
}

/// Validate a contribution registration.
///
/// Returns an error message when the contribution is invalid, `None`
/// otherwise.
pub fn validate_contribution(
    plugin_id: &str,
    contribution_type: &str,
    key: &str,
) -> Option<String> {
    if !is_valid_contribution_type(contribution_type) {
        return Some(format!(
            "plugin '{plugin_id}' attempted to register an unrecognized {contribution_type} contribution"
        ));
    }
    if key.trim().is_empty() {
        return Some(format!(
            "plugin '{plugin_id}' attempted to register a {contribution_type} with an empty key"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_types_are_valid() {
        for t in VALID_CONTRIBUTION_TYPES {
            assert!(is_valid_contribution_type(t), "{t} must be valid");
        }
        assert!(!is_valid_contribution_type("evaluator"));
        assert!(!is_valid_contribution_type(""));
    }

    #[test]
    fn empty_key_is_rejected() {
        let error = validate_contribution("p1", "node-type", "  ").unwrap();
        assert!(error.contains("p1") && error.contains("node-type"));
        assert!(validate_contribution("p1", "node-type", "").is_some());
    }

    #[test]
    fn valid_contribution_passes() {
        assert!(validate_contribution("p1", "tool-type", "my_tool").is_none());
        // Keys are trimmed for the emptiness check only; the original key
        // is registered unchanged.
        assert!(validate_contribution("p1", "event-handler", "on_error").is_none());
    }

    #[test]
    fn unknown_type_is_rejected() {
        let error = validate_contribution("p1", "resource", "r1").unwrap();
        assert!(error.contains("unrecognized"));
    }
}
