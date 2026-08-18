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
    pub fn register(&self, path: PathBuf, hash: String) {
        let now = now_millis();
        self.prune(now);
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&path) {
            if let Some(oldest) = self.oldest_key() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            path,
            AgentWrite {
                hash,
                timestamp: now,
            },
        );
    }

    /// Whether `path`'s current content hash matches a recent agent write
    /// (within the eviction window). This is the deterministic primary
    /// criterion of the manual watcher.
    pub fn is_agent_write(&self, path: &Path, hash: &str) -> bool {
        let now = now_millis();
        self.entries.get(path).is_some_and(|entry| {
            entry.hash == hash && now - entry.timestamp <= self.window.as_millis() as i64
        })
    }

    /// Whether `path` was written by the agent within the grace window.
    /// Watcher events inside the window are skipped unconditionally, covering
    /// the race between the disk write and the registry registration.
    pub fn is_recent_write(&self, path: &Path) -> bool {
        let now = now_millis();
        self.entries
            .get(path)
            .is_some_and(|entry| now - entry.timestamp <= self.grace.as_millis() as i64)
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
            },
        );
        // 500ms is outside the 100ms grace window...
        assert!(!registry.is_recent_write(Path::new("/ws/old.txt")));
        // ...but still inside the 30s eviction window (hash comparison
        // remains the deterministic primary criterion).
        assert!(registry.is_agent_write(Path::new("/ws/old.txt"), "h"));
    }
}
