//! Contribution validation.
//!
//! Contributions are validated before registration: the contribution type
//! must be a recognized [`ContributionType`] and the registration key must
//! be a non-empty string. Validation is chained with the conflict-policy
//! checks performed by the registries.

use std::str::FromStr;

use super::types::ContributionType;

/// Check that a contribution type is recognized.
pub fn is_valid_contribution_type(contribution_type: &str) -> bool {
    ContributionType::from_str(contribution_type).is_ok()
}

/// Validate a contribution registration.
///
/// Returns an error message when the contribution is invalid, `None`
/// otherwise.
pub fn validate_contribution(
    plugin_id: &str,
    contribution_type: ContributionType,
    key: &str,
) -> Option<String> {
    if key.trim().is_empty() {
        return Some(format!(
            "plugin '{plugin_id}' attempted to register a {contribution_type} with an empty key"
        ));
    }
    None
}

/// Validate a raw kebab-case contribution type string (e.g. from a
/// manifest declaration). Returns the parsed type or an error message.
pub fn validate_contribution_type_str(
    plugin_id: &str,
    contribution_type: &str,
) -> Result<ContributionType, String> {
    ContributionType::from_str(contribution_type).map_err(|_| {
        format!(
            "plugin '{plugin_id}' attempted to register an unrecognized {contribution_type} contribution"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_types_are_valid() {
        for t in ContributionType::all() {
            assert!(is_valid_contribution_type(t.as_str()), "{t} must be valid");
        }
        assert!(!is_valid_contribution_type("evaluator"));
        assert!(!is_valid_contribution_type(""));
    }

    #[test]
    fn type_round_trips_through_str() {
        for t in ContributionType::all() {
            assert_eq!(ContributionType::from_str(t.as_str()).unwrap(), *t);
        }
        assert_eq!(
            ContributionType::from_str("tool-type").unwrap(),
            ContributionType::ToolType
        );
    }

    #[test]
    fn empty_key_is_rejected() {
        let error = validate_contribution("p1", ContributionType::NodeType, "  ").unwrap();
        assert!(error.contains("p1") && error.contains("node-type"));
        assert!(validate_contribution("p1", ContributionType::NodeType, "").is_some());
    }

    #[test]
    fn valid_contribution_passes() {
        assert!(validate_contribution("p1", ContributionType::ToolType, "my_tool").is_none());
        // Keys are trimmed for the emptiness check only; the original key
        // is registered unchanged.
        assert!(validate_contribution("p1", ContributionType::EventHandler, "on_error").is_none());
    }

    #[test]
    fn unknown_type_is_rejected() {
        let error = validate_contribution_type_str("p1", "resource").unwrap_err();
        assert!(error.contains("unrecognized"));
    }
}
