//! Chat tab persistence: pinned tab metadata across TUI restarts.
//!
//! Only lightweight metadata is stored (session id, title, pin flag,
//! parent linkage and display hints). Live controllers are rebuilt on
//! startup and each restored tab replays its server history. The file
//! lives under the XDG state directory and a malformed file is ignored
//! so a stale snapshot never breaks startup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Snapshot version. Kept at 1 during development.
pub const TABS_SNAPSHOT_VERSION: u32 = 1;

/// File name inside the state directory.
pub const TABS_FILE_NAME: &str = "tabs.json";

/// Persisted metadata for one tab.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedTab {
    pub session_id: String,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub last_active_ms: u64,
}

/// Persisted tab list snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTabs {
    #[serde(default = "saved_version")]
    pub version: u32,
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
}

fn saved_version() -> u32 {
    TABS_SNAPSHOT_VERSION
}

impl Default for SavedTabs {
    fn default() -> Self {
        Self {
            version: TABS_SNAPSHOT_VERSION,
            active: 0,
            tabs: Vec::new(),
        }
    }
}

/// State directory following XDG conventions (`$XDG_STATE_HOME/wf` or
/// `$HOME/.local/state/wf`).
pub fn state_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|home| home.join(".local/state"))
        })?;
    Some(base.join("wf"))
}

pub fn state_file_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join(TABS_FILE_NAME))
}

/// Load the persisted snapshot. Returns `None` when no snapshot exists
/// or it cannot be parsed.
pub fn load() -> Option<SavedTabs> {
    let path = state_file_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<SavedTabs>(&text).ok()
}

/// Persist `snapshot` atomically. Best effort: failures are silent so a
/// read-only state dir never breaks the TUI.
pub fn save(snapshot: &SavedTabs) {
    let Some(path) = state_file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(text) = serde_json::to_string_pretty(snapshot) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, &path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_tabs_round_trip() {
        let snapshot = SavedTabs {
            version: TABS_SNAPSHOT_VERSION,
            active: 1,
            tabs: vec![
                SavedTab {
                    session_id: "exec-1".to_string(),
                    title: "exec-1".to_string(),
                    pinned: true,
                    parent_id: None,
                    model: Some("claude".to_string()),
                    agent: None,
                    workspace: Some("/repo".to_string()),
                    last_active_ms: 10,
                },
                SavedTab {
                    session_id: "exec-2".to_string(),
                    title: "exec-2".to_string(),
                    pinned: false,
                    parent_id: Some("exec-1".to_string()),
                    model: None,
                    agent: None,
                    workspace: None,
                    last_active_ms: 20,
                },
            ],
        };
        let text = serde_json::to_string(&snapshot).expect("snapshot serializes");
        let back: SavedTabs = serde_json::from_str(&text).expect("snapshot parses");
        assert_eq!(back.active, 1);
        assert_eq!(back.tabs.len(), 2);
        assert!(back.tabs[0].pinned);
        assert_eq!(back.tabs[1].parent_id.as_deref(), Some("exec-1"));
    }

    #[test]
    fn malformed_snapshot_is_rejected_by_caller() {
        let back = serde_json::from_str::<SavedTabs>("{oops");
        assert!(back.is_err());
    }
}
