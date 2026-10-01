use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;

use crate::clock::{clock_valid, CheckpointClock};

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

/// Default capacity of the registry (bound on memory usage).
pub const DEFAULT_CAPACITY: usize = 1024;
/// Default time window after which an entry is considered stale and evicted.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(30);
/// Default grace window after an agent write during which watcher events for
/// the same path are skipped unconditionally (belt-and-braces; the hash
/// comparison is the deterministic primary criterion).
pub const DEFAULT_GRACE: Duration = Duration::from_millis(100);

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
    clock: CheckpointClock,
}

impl RecentAgentWrites {
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_CAPACITY, DEFAULT_WINDOW, DEFAULT_GRACE)
    }

    /// Build the registry with explicit limits: capacity cap + eviction time
    /// window + post-write grace window. Time flows from the system clock.
    pub fn with_limits(capacity: usize, window: Duration, grace: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            capacity: capacity.max(1),
            window,
            grace,
            clock: CheckpointClock::system(),
        }
    }

    /// Drive this registry from an explicit clock (tests advance time
    /// instead of sleeping).
    pub fn with_clock(mut self, clock: CheckpointClock) -> Self {
        self.clock = clock;
        self
    }

    /// Current time, or `None` when the clock is unavailable. Callers fail
    /// closed on `None`: clock failure matches nothing and records nothing.
    fn now(&self) -> Option<i64> {
        self.clock.now_ms().filter(|ms| clock_valid(Some(*ms)))
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
        let Some(now) = self.now() else {
            return;
        };
        self.prune(Some(now));
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
        let Some(now) = self.now() else {
            return;
        };
        self.prune(Some(now));
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
        let Some(now) = self.now() else {
            return false;
        };
        self.entries.get(&key).is_some_and(|entry| {
            entry.deleted
                && now >= entry.timestamp
                && now - entry.timestamp <= self.window.as_millis() as i64
        })
    }

    fn register_inner(&self, key: PathBuf, hash: String, deleted: bool, inflight: bool) {
        let Some(now) = self.now() else {
            return;
        };
        self.prune(Some(now));
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
        let Some(now) = self.now() else {
            return false;
        };
        self.entries.get(&key).is_some_and(|entry| {
            !entry.inflight
                && !entry.deleted
                && entry.hash == hash
                && now >= entry.timestamp
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
        let Some(now) = self.now() else {
            return false;
        };
        self.entries.get(&key).is_some_and(|entry| {
            !entry.inflight
                && !entry.deleted
                && now >= entry.timestamp
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

    /// Remove entries older than the eviction window. A missing clock
    /// prunes nothing.
    pub fn prune(&self, now: Option<i64>) {
        let Some(now) = now.filter(|ms| clock_valid(Some(*ms))) else {
            return;
        };
        let window = self.window.as_millis() as i64;
        self.entries
            .retain(|_, entry| now >= entry.timestamp && now - entry.timestamp <= window);
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
            clock: self.clock.clone(),
        }
    }
}

/// Convenience alias: an `Arc`-shared registry.
pub type SharedRecentAgentWrites = Arc<RecentAgentWrites>;

/// Lexical normalization shared with the watcher: absolute and relative
/// paths fold dot segments without filesystem access so watcher and tool
/// spellings map to the same entry.
fn normalize_key(path: &Path) -> PathBuf {
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
    if path.is_absolute() && out.as_os_str().is_empty() {
        return PathBuf::from("/");
    }
    if out.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;

    const T0: i64 = 1_000_000;

    fn writes() -> (RecentAgentWrites, ManualClock) {
        let clock = CheckpointClock::manual(T0);
        let handle = clock
            .manual_handle()
            .expect("manual clock always has a handle");
        let registry =
            RecentAgentWrites::with_limits(4, Duration::from_secs(30), Duration::from_millis(100))
                .with_clock(clock);
        (registry, handle)
    }

    #[test]
    fn registered_write_matches_its_hash() {
        let (registry, _) = writes();
        registry.register(PathBuf::from("/ws/a.txt"), "hash-a".to_string());
        assert!(registry.is_agent_write(Path::new("/ws/a.txt"), "hash-a"));
        assert!(!registry.is_agent_write(Path::new("/ws/a.txt"), "other"));
        assert!(!registry.is_agent_write(Path::new("/ws/b.txt"), "hash-a"));
    }

    #[test]
    fn register_is_idempotent_per_path() {
        let (registry, _) = writes();
        registry.register(PathBuf::from("/ws/a.txt"), "v1".to_string());
        registry.register(PathBuf::from("/ws/a.txt"), "v2".to_string());
        assert_eq!(registry.len(), 1);
        assert!(registry.is_agent_write(Path::new("/ws/a.txt"), "v2"));
    }

    #[test]
    fn stale_entries_are_pruned() {
        let (registry, handle) = writes();
        registry.register(PathBuf::from("/ws/old.txt"), "old".to_string());
        // 31s later the entry no longer matches and prunes away.
        handle.advance(31_000);
        assert!(!registry.is_agent_write(Path::new("/ws/old.txt"), "old"));
        registry.prune(registry.now());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn capacity_cap_trims_oldest() {
        let (registry, handle) = writes();
        // Distinct timestamps so the oldest-entry eviction is deterministic.
        for i in 0..6 {
            registry.register(PathBuf::from(format!("/ws/f{i}.txt")), format!("h{i}"));
            handle.advance(2);
        }
        assert_eq!(registry.len(), 4);
        // The first two registrations were trimmed.
        assert!(!registry.is_agent_write(Path::new("/ws/f0.txt"), "h0"));
        assert!(!registry.is_agent_write(Path::new("/ws/f1.txt"), "h1"));
        assert!(registry.is_agent_write(Path::new("/ws/f5.txt"), "h5"));
    }

    #[test]
    fn grace_window_skips_recent_writes_regardless_of_hash() {
        let (registry, _) = writes();
        registry.register(PathBuf::from("/ws/a.txt"), "hash-a".to_string());
        assert!(registry.is_recent_write(Path::new("/ws/a.txt")));
        assert!(!registry.is_recent_write(Path::new("/ws/other.txt")));
    }

    #[test]
    fn entries_expire_out_of_the_grace_window() {
        let (registry, handle) = writes();
        registry.register(PathBuf::from("/ws/old.txt"), "h".to_string());
        // 500ms later: outside the 100ms grace window...
        handle.advance(500);
        assert!(!registry.is_recent_write(Path::new("/ws/old.txt")));
        // ...but still inside the 30s eviction window (hash comparison
        // remains the deterministic primary criterion).
        assert!(registry.is_agent_write(Path::new("/ws/old.txt"), "h"));
    }

    #[test]
    fn future_timestamp_entries_never_match() {
        let (registry, _) = writes();
        registry.entries.insert(
            PathBuf::from("/ws/future.txt"),
            AgentWrite {
                hash: "h".to_string(),
                timestamp: T0 + 60_000,
                deleted: false,
                inflight: false,
            },
        );
        assert!(!registry.is_agent_write(Path::new("/ws/future.txt"), "h"));
        assert!(!registry.is_recent_write(Path::new("/ws/future.txt")));
    }

    #[test]
    fn failed_clock_records_nothing_and_matches_nothing() {
        let (registry, handle) = writes();
        handle.fail();
        registry.register(PathBuf::from("/ws/a.txt"), "h".to_string());
        assert_eq!(registry.len(), 0);
        assert!(!registry.is_agent_write(Path::new("/ws/a.txt"), "h"));
        assert!(!registry.is_recent_write(Path::new("/ws/a.txt")));
        registry.prune(None);
        assert_eq!(registry.len(), 0);
        handle.restore();
        registry.register(PathBuf::from("/ws/a.txt"), "h".to_string());
        assert!(registry.is_agent_write(Path::new("/ws/a.txt"), "h"));
    }

    #[test]
    fn relative_spellings_share_one_entry() {
        let (registry, _) = writes();
        registry.register(PathBuf::from("a.txt"), "h".to_string());
        assert!(registry.is_agent_write(Path::new("./a.txt"), "h"));
        assert!(registry.is_recent_write(Path::new("sub/../a.txt")));
    }
}
