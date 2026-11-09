use wf_tools::tool_exposure::{self, ExposureResolution};
use wf_types::tool::Tool;

/// Frozen tool membership for one run, captured at loop entry.
///
/// The snapshot stores the sorted declarations plus stable fingerprints,
/// so a per-turn recomputation can tell intentional loop-boundary changes
/// apart from background registry drift without an external version
/// counter. Fingerprints cover the declaration content (name, description
/// and parameter schema), not just membership, so a same-name schema change
/// also counts as drift. Rendering replays the stored declarations instead
/// of refetching them from the live registry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolSnapshot {
    pub visible: Vec<Tool>,
    pub visible_fingerprint: String,
    pub discoverable: Vec<Tool>,
    pub discoverable_fingerprint: String,
    pub gated: Vec<Tool>,
    pub gated_fingerprint: String,
    pub hidden: Vec<Tool>,
    pub hidden_fingerprint: String,
    pub general_enabled: bool,
}

fn sorted_tools(tools: &[Tool]) -> Vec<Tool> {
    let mut sorted: Vec<Tool> = tools.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    sorted
}

fn sorted_names(tools: &[Tool]) -> Vec<String> {
    let mut names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    names.sort();
    names
}

impl ToolSnapshot {
    pub fn from_resolution(resolution: &ExposureResolution) -> Self {
        let visible = sorted_tools(&resolution.visible);
        let discoverable = sorted_tools(&resolution.discoverable);
        let gated = sorted_tools(&resolution.gated);
        let hidden = sorted_tools(&resolution.hidden);
        Self {
            visible_fingerprint: tool_exposure::fingerprint_declarations(&visible),
            discoverable_fingerprint: tool_exposure::fingerprint_declarations(&discoverable),
            gated_fingerprint: tool_exposure::fingerprint_declarations(&gated),
            hidden_fingerprint: tool_exposure::fingerprint_declarations(&hidden),
            visible,
            discoverable,
            gated,
            hidden,
            general_enabled: resolution.general_enabled,
        }
    }

    pub fn visible_names(&self) -> Vec<String> {
        sorted_names(&self.visible)
    }

    pub fn matches(&self, resolution: &ExposureResolution) -> bool {
        diff_snapshot(self, resolution).is_empty()
    }
}

/// Difference between a frozen snapshot and a live resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolSnapshotDiff {
    pub added_visible: Vec<String>,
    pub removed_visible: Vec<String>,
    pub added_discoverable: Vec<String>,
    pub removed_discoverable: Vec<String>,
    pub added_gated: Vec<String>,
    pub removed_gated: Vec<String>,
    pub added_hidden: Vec<String>,
    pub removed_hidden: Vec<String>,
    /// Tools present on both sides whose declaration content changed
    /// (description or parameter schema updated under the same name).
    pub modified_definitions: Vec<String>,
    pub general_changed: bool,
}

impl ToolSnapshotDiff {
    pub fn is_empty(&self) -> bool {
        self.added_visible.is_empty()
            && self.removed_visible.is_empty()
            && self.added_discoverable.is_empty()
            && self.removed_discoverable.is_empty()
            && self.added_gated.is_empty()
            && self.removed_gated.is_empty()
            && self.added_hidden.is_empty()
            && self.removed_hidden.is_empty()
            && self.modified_definitions.is_empty()
            && !self.general_changed
    }
}

fn diff_names(old: &[String], new: &[String]) -> (Vec<String>, Vec<String>) {
    use std::collections::HashSet;
    let old_set: HashSet<&str> = old.iter().map(String::as_str).collect();
    let new_set: HashSet<&str> = new.iter().map(String::as_str).collect();
    let mut added: Vec<String> = new_set
        .difference(&old_set)
        .map(|s| s.to_string())
        .collect();
    let mut removed: Vec<String> = old_set
        .difference(&new_set)
        .map(|s| s.to_string())
        .collect();
    added.sort();
    removed.sort();
    (added, removed)
}

fn modified_names(old: &[Tool], new: &[Tool]) -> Vec<String> {
    use std::collections::HashMap;
    let old_by_name: HashMap<&str, &Tool> =
        old.iter().map(|tool| (tool.name.as_str(), tool)).collect();
    let mut modified = Vec::new();
    for tool in new {
        if let Some(previous) = old_by_name.get(tool.name.as_str()) {
            if tool_exposure::declaration_fingerprint(previous)
                != tool_exposure::declaration_fingerprint(tool)
            {
                modified.push(tool.name.clone());
            }
        }
    }
    modified.sort();
    modified.dedup();
    modified
}

pub fn diff_snapshot(snapshot: &ToolSnapshot, live: &ExposureResolution) -> ToolSnapshotDiff {
    let live_visible = sorted_names(&live.visible);
    let live_discoverable = sorted_names(&live.discoverable);
    let live_gated = sorted_names(&live.gated);
    let live_hidden = sorted_names(&live.hidden);
    let (added_visible, removed_visible) = diff_names(&snapshot.visible_names(), &live_visible);
    let (added_discoverable, removed_discoverable) =
        diff_names(&snapshot.discoverable_names(), &live_discoverable);
    let (added_gated, removed_gated) = diff_names(&snapshot.gated_names(), &live_gated);
    let (added_hidden, removed_hidden) = diff_names(&snapshot.hidden_names(), &live_hidden);
    let mut modified_definitions = modified_names(&snapshot.visible, &live.visible);
    modified_definitions.extend(modified_names(&snapshot.discoverable, &live.discoverable));
    modified_definitions.extend(modified_names(&snapshot.gated, &live.gated));
    modified_definitions.extend(modified_names(&snapshot.hidden, &live.hidden));
    modified_definitions.sort();
    modified_definitions.dedup();
    ToolSnapshotDiff {
        added_visible,
        removed_visible,
        added_discoverable,
        removed_discoverable,
        added_gated,
        removed_gated,
        added_hidden,
        removed_hidden,
        modified_definitions,
        general_changed: snapshot.general_enabled != live.general_enabled,
    }
}

impl ToolSnapshot {
    fn discoverable_names(&self) -> Vec<String> {
        sorted_names(&self.discoverable)
    }

    fn gated_names(&self) -> Vec<String> {
        sorted_names(&self.gated)
    }

    fn hidden_names(&self) -> Vec<String> {
        sorted_names(&self.hidden)
    }
}

/// Render an explicit tool change notice. Returns `None` when there is
/// nothing to announce. Kept as plain text; the caller decides the bearing
/// message.
pub fn render_tool_change_text(
    diff: &ToolSnapshotDiff,
    activated: &[String],
    blocked: &[String],
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if !diff.added_visible.is_empty() {
        lines.push(format!(
            "Newly available tools (now directly callable): {}",
            diff.added_visible.join(", ")
        ));
    }
    if !diff.removed_visible.is_empty() {
        lines.push(format!(
            "Removed tools (no longer callable, do not retry): {}",
            diff.removed_visible.join(", ")
        ));
    }
    if !diff.added_discoverable.is_empty() {
        lines.push(format!(
            "Newly discoverable tools (invoke through general): {}",
            diff.added_discoverable.join(", ")
        ));
    }
    if !diff.removed_discoverable.is_empty() {
        lines.push(format!(
            "No longer discoverable tools: {}",
            diff.removed_discoverable.join(", ")
        ));
    }
    if !diff.added_gated.is_empty() {
        lines.push(format!(
            "Newly gated tools (require explicit activation): {}",
            diff.added_gated.join(", ")
        ));
    }
    if !diff.removed_gated.is_empty() {
        lines.push(format!(
            "No longer gated tools: {}",
            diff.removed_gated.join(", ")
        ));
    }
    if !diff.added_hidden.is_empty() {
        lines.push(format!(
            "Newly hidden tools (never exposed): {}",
            diff.added_hidden.join(", ")
        ));
    }
    if !diff.removed_hidden.is_empty() {
        lines.push(format!(
            "No longer hidden tools: {}",
            diff.removed_hidden.join(", ")
        ));
    }
    if !diff.modified_definitions.is_empty() {
        lines.push(format!(
            "Changed tool definitions (schema updated, review usage before calling): {}",
            diff.modified_definitions.join(", ")
        ));
    }
    if diff.general_changed {
        lines.push("The general proxy availability changed.".to_string());
    }
    let mut activated_sorted = activated.to_vec();
    activated_sorted.sort();
    activated_sorted.dedup();
    if !activated_sorted.is_empty() {
        lines.push(format!(
            "Formally activated tools (now directly callable): {}",
            activated_sorted.join(", ")
        ));
    }
    let mut blocked_sorted = blocked.to_vec();
    blocked_sorted.sort();
    blocked_sorted.dedup();
    if !blocked_sorted.is_empty() {
        lines.push(format!(
            "Blocked tools (declared but not executable, do not retry): {}",
            blocked_sorted.join(", ")
        ));
    }
    if lines.is_empty() {
        return None;
    }
    let mut text = String::from("[Tool Change]");
    for line in lines {
        text.push_str("\n- ");
        text.push_str(&line);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> Tool {
        Tool {
            id: name.into(),
            name: name.into(),
            description: format!("tool {name}"),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn resolution(
        visible: Vec<&str>,
        discoverable: Vec<&str>,
        general: bool,
    ) -> ExposureResolution {
        ExposureResolution {
            visible: visible.into_iter().map(tool).collect(),
            discoverable: discoverable.into_iter().map(tool).collect(),
            gated: Vec::new(),
            hidden: Vec::new(),
            general_enabled: general,
        }
    }

    #[test]
    fn snapshot_roundtrip_matches() {
        let res = resolution(vec!["b", "a"], vec!["c"], true);
        let snap = ToolSnapshot::from_resolution(&res);
        assert_eq!(snap.visible_names(), vec!["a".to_string(), "b".to_string()]);
        assert!(snap.matches(&res));
    }

    #[test]
    fn snapshot_freezes_declarations_not_names() {
        let res = resolution(vec!["a"], vec![], false);
        let snap = ToolSnapshot::from_resolution(&res);
        assert_eq!(snap.visible.len(), 1);
        assert_eq!(snap.visible[0].description, "tool a");
    }

    #[test]
    fn diff_detects_add_remove() {
        let old = ToolSnapshot::from_resolution(&resolution(vec!["a"], vec![], false));
        let live = resolution(vec!["a", "b"], vec!["c"], true);
        let diff = diff_snapshot(&old, &live);
        assert_eq!(diff.added_visible, vec!["b".to_string()]);
        assert_eq!(diff.added_discoverable, vec!["c".to_string()]);
        assert!(diff.general_changed);
        assert!(!diff.is_empty());
    }

    #[test]
    fn diff_detects_definition_change_under_same_name() {
        let old = ToolSnapshot::from_resolution(&resolution(vec!["a"], vec![], false));
        let mut live = resolution(vec!["a"], vec![], false);
        live.visible[0].description = "changed description".to_string();
        let diff = diff_snapshot(&old, &live);
        assert!(diff.added_visible.is_empty());
        assert!(diff.removed_visible.is_empty());
        assert_eq!(diff.modified_definitions, vec!["a".to_string()]);
        assert!(!diff.is_empty());
        assert!(!old.matches(&live));
    }

    #[test]
    fn snapshot_covers_gated_and_hidden() {
        let mut res = resolution(vec!["a"], vec![], false);
        res.gated.push(tool("g"));
        res.hidden.push(tool("h"));
        let snap = ToolSnapshot::from_resolution(&res);
        assert_eq!(snap.gated_names(), vec!["g".to_string()]);
        assert_eq!(snap.hidden_names(), vec!["h".to_string()]);
        assert!(snap.matches(&res));
        let live = resolution(vec!["a"], vec![], false);
        let diff = diff_snapshot(&snap, &live);
        assert_eq!(diff.removed_gated, vec!["g".to_string()]);
        assert_eq!(diff.removed_hidden, vec!["h".to_string()]);
        assert!(!diff.is_empty());
    }

    #[test]
    fn render_is_none_when_empty() {
        let diff = ToolSnapshotDiff::default();
        assert!(render_tool_change_text(&diff, &[], &[]).is_none());
    }

    #[test]
    fn render_lists_blocked_with_do_not_retry() {
        let diff = ToolSnapshotDiff::default();
        let text =
            render_tool_change_text(&diff, &[], &["shell".to_string()]).expect("blocked notice");
        assert!(text.contains("Blocked tools"));
        assert!(text.contains("do not retry"));
        assert!(text.contains("shell"));
    }
}
