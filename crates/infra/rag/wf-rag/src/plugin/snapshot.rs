use super::*;

fn snapshot_registry() -> &'static Mutex<HashMap<String, Arc<serde_json::Value>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<serde_json::Value>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stores a snapshot expectation document under `name`.
///
/// Reservation only: no pipeline reads stored policies. Execution stays
/// host-owned; plugins only declare expectations for operators to compare.
pub fn register_snapshot_policy(name: &str, policy: Arc<serde_json::Value>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = snapshot_registry().lock() {
        guard.insert(trimmed.to_string(), policy);
    }
}

/// Removes a snapshot expectation document.
pub fn unregister_snapshot_policy(name: &str) {
    if let Ok(mut guard) = snapshot_registry().lock() {
        guard.remove(name.trim());
    }
}

/// Finds a snapshot expectation document by name.
pub fn find_snapshot_policy(name: &str) -> Option<Arc<serde_json::Value>> {
    snapshot_registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears snapshot policies; tests use it to isolate cases.
pub fn clear_snapshot_policies() {
    if let Ok(mut guard) = snapshot_registry().lock() {
        guard.clear();
    }
}

/// Whether a snapshot policy document carries an object shape.
pub fn is_valid_snapshot_policy(policy: &serde_json::Value) -> bool {
    policy.is_object()
}
