use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use wf_tools::tool_exposure::ExposureResolution;
use wf_types::tool::Tool;

/// Frozen tool membership for one run, captured at loop entry.
///
/// The snapshot stores sorted names plus hashes, so a per-turn recomputation
/// can tell intentional loop-boundary changes apart from background registry
/// drift without an external version counter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolSnapshot {
    pub visible_names: Vec<String>,
    pub visible_hash: u64,
    pub discoverable_names: Vec<String>,
    pub discoverable_hash: u64,
    pub gated_names: Vec<String>,
    pub gated_hash: u64,
    pub hidden_names: Vec<String>,
    pub hidden_hash: u64,
    pub general_enabled: bool,
}

fn fingerprint_names(names: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    names.len().hash(&mut hasher);
    for name in names {
        name.hash(&mut hasher);
    }
    hasher.finish()
}

fn sorted_names(tools: &[Tool]) -> Vec<String> {
    let mut names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    names.sort();
    names
}

impl ToolSnapshot {
    pub fn from_resolution(resolution: &ExposureResolution) -> Self {
        let visible_names = sorted_names(&resolution.visible);
        let discoverable_names = sorted_names(&resolution.discoverable);
        let gated_names = sorted_names(&resolution.gated);
        let hidden_names = sorted_names(&resolution.hidden);
        let visible_hash = fingerprint_names(&visible_names);
        let discoverable_hash = fingerprint_names(&discoverable_names);
        let gated_hash = fingerprint_names(&gated_names);
        let hidden_hash = fingerprint_names(&hidden_names);
        Self {
            visible_names,
            visible_hash,
            discoverable_names,
            discoverable_hash,
            gated_names,
            gated_hash,
            hidden_names,
            hidden_hash,
            general_enabled: resolution.general_enabled,
        }
    }

    pub fn matches(&self, resolution: &ExposureResolution) -> bool {
        let other = Self::from_resolution(resolution);
        self.visible_hash == other.visible_hash
            && self.discoverable_hash == other.discoverable_hash
            && self.gated_hash == other.gated_hash
            && self.hidden_hash == other.hidden_hash
            && self.general_enabled == other.general_enabled
            && self.visible_names == other.visible_names
            && self.discoverable_names == other.discoverable_names
            && self.gated_names == other.gated_names
            && self.hidden_names == other.hidden_names
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

pub fn diff_snapshot(snapshot: &ToolSnapshot, live: &ExposureResolution) -> ToolSnapshotDiff {
    let live_visible = sorted_names(&live.visible);
    let live_discoverable = sorted_names(&live.discoverable);
    let live_gated = sorted_names(&live.gated);
    let live_hidden = sorted_names(&live.hidden);
    let (added_visible, removed_visible) = diff_names(&snapshot.visible_names, &live_visible);
    let (added_discoverable, removed_discoverable) =
        diff_names(&snapshot.discoverable_names, &live_discoverable);
    let (added_gated, removed_gated) = diff_names(&snapshot.gated_names, &live_gated);
    let (added_hidden, removed_hidden) = diff_names(&snapshot.hidden_names, &live_hidden);
    ToolSnapshotDiff {
        added_visible,
        removed_visible,
        added_discoverable,
        removed_discoverable,
        added_gated,
        removed_gated,
        added_hidden,
        removed_hidden,
        general_changed: snapshot.general_enabled != live.general_enabled,
    }
}

/// Render an explicit tool change notice. Returns `None` when there is
/// nothing to announce. Kept as plain text; the caller decides the bearing
/// system message.
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
        assert_eq!(snap.visible_names, vec!["a".to_string(), "b".to_string()]);
        assert!(snap.matches(&res));
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
    fn snapshot_covers_gated_and_hidden() {
        let mut res = resolution(vec!["a"], vec![], false);
        res.gated.push(tool("g"));
        res.hidden.push(tool("h"));
        let snap = ToolSnapshot::from_resolution(&res);
        assert_eq!(snap.gated_names, vec!["g".to_string()]);
        assert_eq!(snap.hidden_names, vec!["h".to_string()]);
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
