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
    #[serde(default = "default_true")]
    pub title_is_default: bool,
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

fn default_true() -> bool {
    true
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

/// One client-side tree bookmark: a pinned node inside a session tree.
/// Bookmarks never touch the server model; the tree resolves them by node
/// id when the session is reopened, and stale ids simply match nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedBookmark {
    pub session_id: String,
    pub node_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub created_ms: u64,
}

/// Persisted bookmark collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedBookmarks {
    #[serde(default = "saved_version")]
    pub version: u32,
    #[serde(default)]
    pub bookmarks: Vec<SavedBookmark>,
}

/// File name for tree bookmarks inside the state directory.
pub const BOOKMARKS_FILE_NAME: &str = "bookmarks.json";

pub fn bookmarks_file_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join(BOOKMARKS_FILE_NAME))
}

/// Load persisted bookmarks. Returns empty when none exist or the file
/// cannot be parsed, so a stale file never breaks the tree.
pub fn load_bookmarks() -> SavedBookmarks {
    let Some(path) = bookmarks_file_path() else {
        return SavedBookmarks::default();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return SavedBookmarks::default();
    };
    serde_json::from_str::<SavedBookmarks>(&text).unwrap_or_default()
}

/// Persist bookmarks atomically. Best effort and silent like tab saving.
pub fn save_bookmarks(bookmarks: &SavedBookmarks) {
    let Some(path) = bookmarks_file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(text) = serde_json::to_string_pretty(bookmarks) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, &path);
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
                    title_is_default: true,
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
                    title_is_default: false,
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

    #[test]
    fn bookmarks_round_trip_with_defaults() {
        let saved = SavedBookmarks {
            version: TABS_SNAPSHOT_VERSION,
            bookmarks: vec![SavedBookmark {
                session_id: "exec-1".to_string(),
                node_id: "m-3".to_string(),
                label: "read the file".to_string(),
                created_ms: 42,
            }],
        };
        let text = serde_json::to_string(&saved).expect("bookmarks serialize");
        let back: SavedBookmarks = serde_json::from_str(&text).expect("bookmarks parse");
        assert_eq!(back.bookmarks.len(), 1);
        assert_eq!(back.bookmarks[0].node_id, "m-3");
        // Missing fields default so older files keep loading.
        let legacy: SavedBookmarks =
            serde_json::from_str(r#"{"bookmarks": [{"session_id": "e", "node_id": "n"}]}"#)
                .expect("legacy bookmarks parse");
        assert_eq!(legacy.bookmarks[0].label, "");
        assert_eq!(legacy.bookmarks[0].created_ms, 0);
    }
}
