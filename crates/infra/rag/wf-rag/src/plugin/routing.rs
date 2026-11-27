use super::*;

/// Runs one route selector and returns the trimmed non-empty target
/// suggestions without validation or fallback. Execution failures surface
/// as errors so each caller applies its own fallback (candidate list or
/// keyword rules) from a single parsing path.
pub(crate) fn poll_route_selector(
    selector: &dyn PluginRouteSelector,
    query: &str,
    candidates: &[String],
) -> wf_plugin_sdk::PluginResult<Vec<String>> {
    let input = serde_json::json!({
        "protocol_version": ROUTE_SELECTOR_PROTOCOL_VERSION,
        "query": query,
        "candidates": candidates,
    });
    let output = selector.select(input)?;
    Ok(output
        .get("targets")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}
