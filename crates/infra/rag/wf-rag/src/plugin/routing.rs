use super::*;

fn selector_registry() -> &'static Mutex<HashMap<String, Arc<dyn PluginRouteSelector>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<dyn PluginRouteSelector>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a route selector under `name`, replacing any prior entry.
pub fn register_route_selector(name: &str, selector: Arc<dyn PluginRouteSelector>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = selector_registry().lock() {
        guard.insert(trimmed.to_string(), selector);
    }
}

/// Removes a route selector.
pub fn unregister_route_selector(name: &str) {
    if let Ok(mut guard) = selector_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds a registered route selector by name.
pub fn find_route_selector(name: &str) -> Option<Arc<dyn PluginRouteSelector>> {
    selector_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears route selectors; tests use it to isolate cases.
pub fn clear_route_selectors() {
    if let Ok(mut guard) = selector_registry().lock() {
        guard.clear();
    }
}

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

/// Selects retrieval targets through the named selector.
/// Unknown names fail; unknown targets are skipped and an empty
/// suggestion defers to the full candidate list.
pub fn apply_registered_selector(
    query: &str,
    candidates: &[String],
    name: Option<&str>,
) -> crate::error::Result<Vec<String>> {
    let Some(name) = name.filter(|name| !name.trim().is_empty()) else {
        return Ok(candidates.to_vec());
    };
    let Some(selector) = find_route_selector(name) else {
        return Err(crate::error::RagError::InvalidInput(format!(
            "unknown route selector '{name}'"
        )));
    };
    let selected = match poll_route_selector(selector.as_ref(), query, candidates) {
        Ok(selected) => selected,
        Err(error) => {
            tracing::warn!(error = %error, "route selector failed; keeping all targets");
            return Ok(candidates.to_vec());
        }
    };
    if selected.is_empty() {
        return Ok(candidates.to_vec());
    }
    let kept: Vec<String> = selected
        .into_iter()
        .filter(|item| candidates.iter().any(|candidate| candidate == item))
        .collect();
    if kept.is_empty() {
        return Ok(candidates.to_vec());
    }
    Ok(kept)
}
