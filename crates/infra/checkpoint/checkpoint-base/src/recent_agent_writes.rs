use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

/// A registered agent write: the content hash written to `path` at
/// `timestamp` (Unix milliseconds).
#[derive(Debug, Clone)]
struct AgentWrite {
    hash: String,
    timestamp: i64,
    /// Explicit deletion marker: the agent deleted this path (empty content
    /// is not a reliable signal on its own).
    deleted: bool,
    /// In-flight scope lease: a scoped execution (shell diff) has started
    /// and may still write to this path. Watcher events under lease must be
    /// deferred, not permanently dropped.
    inflight: bool,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Default capacity of the registry (bound on memory usage).
const DEFAULT_CAPACITY: usize = 1024;
/// Default time window after which an entry is considered stale and evicted.
const DEFAULT_WINDOW: Duration = Duration::from_secs(30);
/// Default grace window after an agent write during which watcher events for
/// the same path are skipped unconditionally (belt-and-braces; the hash
/// comparison is the deterministic primary criterion).
const DEFAULT_GRACE: Duration = Duration::from_millis(100);

/// Registry of recent agent-written file hashes (path -> content sha256).
///
/// The manual watcher uses it to tell "who made this change" apart: when a
/// watcher event fires for a path whose current content hash matches a
/// recently registered agent write, the change is the agent's own write and
/// must be skipped (it was already recorded via `apply_agent_edit`).
/// Entries are evicted by a time window and a capacity cap.
pub struct RecentAgentWrites {
    entries: DashMap<PathBuf, AgentWrite>,
    capacity: usize,
    window: Duration,
    grace: Duration,
}

impl RecentAgentWrites {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_CAPACITY, DEFAULT_WINDOW, DEFAULT_GRACE)
    }

    /// Build the registry with explicit limits: capacity cap + eviction time
    /// window + post-write grace window.
    pub fn with_limits(capacity: usize, window: Duration, grace: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            capacity: capacity.max(1),
            window,
            grace,
        }
    }

    /// Register an agent write. The timestamp is taken now; expired entries
    /// are pruned and the registry is trimmed to its capacity cap.
    /// All keys are lexically normalized so watcher (absolute) and tool
    /// (relative/absolute) spellings map to the same entry.
    pub fn register(&self, path: PathBuf, hash: String) {
        self.register_inner(normalize_key(&path), hash, false, false);
    }

    /// Register an explicit agent deletion. Deletion attribution must use
    /// this marker plus path identity, never the grace window alone.
    pub fn register_delete(&self, path: PathBuf) {
        self.register_inner(normalize_key(&path), String::new(), true, false);
    }

    /// Acquire an in-flight scope lease for a path that a scoped execution
    /// may still write to. Watcher hits under lease should be deferred.
    pub fn acquire_inflight(&self, path: PathBuf) {
        let key = normalize_key(&path);
        let now = now_millis();
        self.prune(now);
        if let Some(mut entry) = self.entries.get_mut(&key) {
            entry.inflight = true;
            entry.timestamp = now;
        } else {
            if self.entries.len() >= self.capacity {
                if let Some(oldest) = self.oldest_key() {
                    self.entries.remove(&oldest);
                }
            }
            self.entries.insert(
                key,
                AgentWrite {
                    hash: String::new(),
                    timestamp: now,
                    deleted: false,
                    inflight: true,
                },
            );
        }
    }

    /// Resolve an in-flight lease after the scoped execution sampled its
    /// final content: records the final hash and clears the lease. Pass
    /// `deleted=true` when the path no longer exists.
    pub fn resolve_inflight(&self, path: PathBuf, hash: String, deleted: bool) {
        let key = normalize_key(&path);
        let now = now_millis();
        self.prune(now);
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            if let Some(oldest) = self.oldest_key() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            AgentWrite {
                hash,
                timestamp: now,
                deleted,
                inflight: false,
            },
        );
    }

    /// Whether the path currently holds an in-flight scope lease.
    pub fn is_inflight(&self, path: &Path) -> bool {
        let key = normalize_key(path);
        self.entries.get(&key).is_some_and(|entry| entry.inflight)
    }

    /// Whether the path was explicitly deleted by the agent (within the
    /// eviction window). Used for delete attribution instead of the grace
    /// window.
    pub fn is_agent_delete(&self, path: &Path) -> bool {
        let key = normalize_key(path);
        let now = now_millis();
        self.entries.get(&key).is_some_and(|entry| {
            entry.deleted && now - entry.timestamp <= self.window.as_millis() as i64
        })
    }

    fn register_inner(&self, key: PathBuf, hash: String, deleted: bool, inflight: bool) {
        let now = now_millis();
        self.prune(now);
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            if let Some(oldest) = self.oldest_key() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            AgentWrite {
                hash,
                timestamp: now,
                deleted,
                inflight,
            },
        );
    }

    /// Whether `path`'s current content hash matches a recent agent write
    /// (within the eviction window). This is the deterministic primary
    /// criterion of the manual watcher. In-flight leases never count as a
    /// hash match: their content is not final yet.
    pub fn is_agent_write(&self, path: &Path, hash: &str) -> bool {
        let key = normalize_key(path);
        let now = now_millis();
        self.entries.get(&key).is_some_and(|entry| {
            !entry.inflight
                && !entry.deleted
                && entry.hash == hash
                && now - entry.timestamp <= self.window.as_millis() as i64
        })
    }

    /// Whether `path` was written by the agent within the grace window.
    /// Covers only the race between the disk write and the hash
    /// registration for add/modify events; must not be used for delete
    /// attribution (use [`Self::is_agent_delete`]) and never matches a path
    /// that only holds an in-flight lease.
    pub fn is_recent_write(&self, path: &Path) -> bool {
        let key = normalize_key(path);
        let now = now_millis();
        self.entries.get(&key).is_some_and(|entry| {
            !entry.inflight
                && !entry.deleted
                && now - entry.timestamp <= self.grace.as_millis() as i64
        })
    }

    /// Number of tracked entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove entries older than the eviction window.
    pub fn prune(&self, now: i64) {
        let window = self.window.as_millis() as i64;
        self.entries
            .retain(|_, entry| now - entry.timestamp <= window);
    }

    fn oldest_key(&self) -> Option<PathBuf> {
        self.entries
            .iter()
            .min_by_key(|e| e.value().timestamp)
            .map(|e| e.key().clone())
    }
}

impl Default for RecentAgentWrites {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for RecentAgentWrites {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            capacity: self.capacity,
            window: self.window,
            grace: self.grace,
        }
    }
}

/// Convenience alias: an `Arc`-shared registry.
pub type SharedRecentAgentWrites = Arc<RecentAgentWrites>;

/// Lexical normalization shared with the watcher: absolute paths are
/// normalized without filesystem access; relative paths are kept as-is
/// (callers register both spellings for agent edits).
fn normalize_key(path: &Path) -> PathBuf {
    if path.is_absolute() {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    out.pop();
                }
                other => out.push(other.as_os_str()),
            }
        }
        if out.as_os_str().is_empty() {
            return PathBuf::from("/");
        }
        out
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writes() -> RecentAgentWrites {
        RecentAgentWrites::with_limits(4, Duration::from_secs(30), Duration::from_millis(100))
    }

    #[test]
    fn registered_write_matches_its_hash() {
        let registry = writes();
        registry.register(PathBuf::from("/ws/a.txt"), "hash-a".to_string());
        assert!(registry.is_agent_write(Path::new("/ws/a.txt"), "hash-a"));
        assert!(!registry.is_agent_write(Path::new("/ws/a.txt"), "other"));
        assert!(!registry.is_agent_write(Path::new("/ws/b.txt"), "hash-a"));
    }

    #[test]
    fn register_is_idempotent_per_path() {
        let registry = writes();
        registry.register(PathBuf::from("/ws/a.txt"), "v1".to_string());
        registry.register(PathBuf::from("/ws/a.txt"), "v2".to_string());
        assert_eq!(registry.len(), 1);
        assert!(registry.is_agent_write(Path::new("/ws/a.txt"), "v2"));
    }

    #[test]
    fn stale_entries_are_pruned() {
        let registry = writes();
        registry.register(PathBuf::from("/ws/old.txt"), "old".to_string());
        let now = now_millis();
        // Pretend 31s passed: the entry no longer matches. The entry is
        // overwritten directly (holding a DashMap ref across the insert
        // would deadlock the shard).
        registry.entries.insert(
            PathBuf::from("/ws/old.txt"),
            AgentWrite {
                hash: "old".to_string(),
                timestamp: now - 31_000,
                deleted: false,
                inflight: false,
            },
        );
        assert!(!registry.is_agent_write(Path::new("/ws/old.txt"), "old"));
        registry.prune(now);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn capacity_cap_trims_oldest() {
        let registry = writes();
        // Distinct timestamps so the oldest-entry eviction is deterministic
        // (same-millisecond registrations make `oldest_key` arbitrary).
        for i in 0..6 {
            registry.register(PathBuf::from(format!("/ws/f{i}.txt")), format!("h{i}"));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(registry.len(), 4);
        // The first two registrations were trimmed.
        assert!(!registry.is_agent_write(Path::new("/ws/f0.txt"), "h0"));
        assert!(!registry.is_agent_write(Path::new("/ws/f1.txt"), "h1"));
        assert!(registry.is_agent_write(Path::new("/ws/f5.txt"), "h5"));
    }

    #[test]
    fn grace_window_skips_recent_writes_regardless_of_hash() {
        let registry = writes();
        registry.register(PathBuf::from("/ws/a.txt"), "hash-a".to_string());
        assert!(registry.is_recent_write(Path::new("/ws/a.txt")));
        assert!(!registry.is_recent_write(Path::new("/ws/other.txt")));
    }

    #[test]
    fn entries_expire_out_of_the_grace_window() {
        let registry = writes();
        let now = now_millis();
        registry.entries.insert(
            PathBuf::from("/ws/old.txt"),
            AgentWrite {
                hash: "h".to_string(),
                timestamp: now - 500,
                deleted: false,
                inflight: false,
            },
        );
        // 500ms is outside the 100ms grace window...
        assert!(!registry.is_recent_write(Path::new("/ws/old.txt")));
        // ...but still inside the 30s eviction window (hash comparison
        // remains the deterministic primary criterion).
        assert!(registry.is_agent_write(Path::new("/ws/old.txt"), "h"));
    }
}
