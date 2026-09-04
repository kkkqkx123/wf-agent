use crate::error::ConfigResult;
use crate::validator::validate_no_intersection;

/// Validate that tool list partitions do not intersect.
///
/// Checks `available` vs `hidden`, `available` vs `discoverable`, and
/// `discoverable` vs `hidden`. Shared by agent and workflow definition
/// validation (single source of truth for partition semantics).
pub fn validate_tool_list_intersection(
    available: &[String],
    hidden: Option<&[String]>,
    discoverable: Option<&[String]>,
    field_prefix: &str,
) -> ConfigResult<()> {
    if let Some(hidden) = hidden {
        validate_no_intersection(
            available,
            hidden,
            &format!("{field_prefix}.available"),
            &format!("{field_prefix}.hidden"),
        )?;
    }
    if let Some(discoverable) = discoverable {
        validate_no_intersection(
            available,
            discoverable,
            &format!("{field_prefix}.available"),
            &format!("{field_prefix}.discoverable"),
        )?;
        if let Some(hidden) = hidden {
            validate_no_intersection(
                discoverable,
                hidden,
                &format!("{field_prefix}.discoverable"),
                &format!("{field_prefix}.hidden"),
            )?;
        }
    }
    Ok(())
}

/// Validate an `AvailableTools` value against the partition rules using
/// `field_prefix` (for example `config.available_tools`) in error paths.
pub fn validate_available_tools(
    tools: &wf_types::tool::AvailableTools,
    field_prefix: &str,
) -> ConfigResult<()> {
    validate_tool_list_intersection(
        &tools.available,
        tools.hidden.as_deref(),
        tools.discoverable.as_deref(),
        field_prefix,
    )
}
