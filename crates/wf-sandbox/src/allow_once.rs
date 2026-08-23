//! Allow-once temporary exceptions (HMAC-style short codes).
//!
//! When a command is denied, the
//! decision layer can issue a short code that lets the user approve exactly
//! that command — once, or repeatedly within a TTL window — without touching
//! the global policy. Codes are deterministic (keyed) short digests, not
//! guessable sequence numbers, and are scoped to a cwd/project context.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Default lifetime of an allow-once exception (24 hours).
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 3600);
/// Short-code length in hex characters.
pub const CODE_LEN: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct AllowOnceEntry {
    code: String,
    scope: String,
    command_fingerprint: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
    single_use: bool,
    used: bool,
}

/// Outcome of redeeming an allow-once code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemResult {
    /// The command is approved and (for single-use codes) consumed.
    Approved,
    /// No such code.
    NotFound,
    /// Code exists but the command does not match the issued one.
    CommandMismatch,
    /// Code belongs to a different cwd/project scope.
    ScopeMismatch,
    /// Code has expired.
    Expired,
    /// Single-use code already consumed.
    AlreadyUsed,
}

/// In-memory store of pending allow-once exceptions with optional JSONL
/// persistence.
pub struct AllowOnceStore {
    secret: [u8; 32],
    ttl: Duration,
    scope: String,
    entries: Mutex<HashMap<String, AllowOnceEntry>>,
    jsonl_path: Option<std::path::PathBuf>,
    nonce: std::sync::atomic::AtomicU64,
}

impl std::fmt::Debug for AllowOnceStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AllowOnceStore")
            .field("scope", &self.scope)
            .field("ttl_secs", &self.ttl.as_secs())
            .field("jsonl_path", &self.jsonl_path)
            .field(
                "pending",
                &self.entries.lock().map(|e| e.len()).unwrap_or(0),
            )
            .finish_non_exhaustive()
    }
}

impl AllowOnceStore {
    pub fn new(secret: [u8; 32], scope: impl Into<String>) -> Self {
        Self {
            secret,
            ttl: DEFAULT_TTL,
            scope: scope.into(),
            entries: Mutex::new(HashMap::new()),
            jsonl_path: None,
            nonce: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Configure the exception lifetime (used by tests to exercise expiry).
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Attach a JSONL file for persistence. `load` must be called separately
    /// to hydrate the store from disk.
    pub fn with_jsonl(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.jsonl_path = Some(path.into());
        self
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Issue a new allow-once code for `command`. Returns the short code.
    /// The same command may be issued multiple times (each yields a distinct
    /// code via a nonce).
    pub fn issue(&self, command: &str) -> String {
        let now = now_ms();
        let nonce = self
            .nonce
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let code = self.derive_code(command, nonce);
        let fingerprint = self.fingerprint(command);
        let entry = AllowOnceEntry {
            code: code.clone(),
            scope: self.scope.clone(),
            command_fingerprint: fingerprint,
            issued_at_ms: now,
            expires_at_ms: now + self.ttl.as_millis() as i64,
            single_use: true,
            used: false,
        };
        self.entries.lock().unwrap().insert(code.clone(), entry);
        code
    }

    /// Attempt to redeem `code` for `command` in the store's scope.
    pub fn redeem(&self, code: &str, command: &str) -> RedeemResult {
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries.get_mut(code) else {
            return RedeemResult::NotFound;
        };
        if entry.scope != self.scope {
            return RedeemResult::ScopeMismatch;
        }
        if now_ms() > entry.expires_at_ms {
            entries.remove(code);
            return RedeemResult::Expired;
        }
        if self.fingerprint(command) != entry.command_fingerprint {
            return RedeemResult::CommandMismatch;
        }
        if entry.single_use {
            if entry.used {
                return RedeemResult::AlreadyUsed;
            }
            entry.used = true;
        }
        RedeemResult::Approved
    }

    /// Number of pending (non-expired) exceptions.
    pub fn pending_count(&self) -> usize {
        let mut entries = self.entries.lock().unwrap();
        let now = now_ms();
        entries.retain(|_, e| e.expires_at_ms >= now);
        entries.len()
    }

    /// Persist all entries as JSONL (one entry per line) to the configured
    /// path, or to `path` when provided.
    pub fn persist(&self, path: &Path) -> std::io::Result<usize> {
        let entries = self.entries.lock().unwrap();
        let mut out = String::new();
        for e in entries.values() {
            out.push_str(&serde_json::to_string(e).map_err(io_err)?);
            out.push('\n');
        }
        std::fs::write(path, out)?;
        Ok(entries.len())
    }

    /// Hydrate the store from a JSONL file (skipping malformed lines).
    /// Returns the number of entries loaded.
    pub fn load(&self, path: &Path) -> std::io::Result<usize> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        let mut entries = self.entries.lock().unwrap();
        let mut count = 0;
        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<AllowOnceEntry>(line) {
                if entry.scope == self.scope && entry.expires_at_ms >= now_ms() {
                    entries.insert(entry.code.clone(), entry);
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Append a single issued entry to the JSONL log (best-effort).
    pub fn append_log(&self) {
        if let Some(path) = &self.jsonl_path {
            let entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.values().max_by_key(|e| e.issued_at_ms) {
                if let Ok(line) = serde_json::to_string(entry) {
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .and_then(|mut f| {
                            use std::io::Write;
                            writeln!(f, "{line}")
                        });
                }
            }
        }
    }

    fn fingerprint(&self, command: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.secret);
        hasher.update(self.scope.as_bytes());
        hasher.update(command.as_bytes());
        hasher.finalize().to_hex()[..32].to_string()
    }

    fn derive_code(&self, command: &str, nonce: u64) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.secret);
        hasher.update(self.scope.as_bytes());
        hasher.update(command.as_bytes());
        hasher.update(&nonce.to_le_bytes());
        hasher.finalize().to_hex()[..CODE_LEN].to_string()
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(scope: &str) -> AllowOnceStore {
        AllowOnceStore::new([7u8; 32], scope)
    }

    #[test]
    fn test_issue_and_redeem() {
        let s = store("proj-a");
        let code = s.issue("rm -rf /tmp/cache");
        assert_eq!(code.len(), CODE_LEN);
        assert_eq!(s.redeem(&code, "rm -rf /tmp/cache"), RedeemResult::Approved);
        // Single-use: a second redeem is rejected.
        assert_eq!(
            s.redeem(&code, "rm -rf /tmp/cache"),
            RedeemResult::AlreadyUsed
        );
    }

    #[test]
    fn test_command_mismatch() {
        let s = store("proj-a");
        let code = s.issue("rm -rf /tmp/cache");
        assert_eq!(
            s.redeem(&code, "rm -rf /other"),
            RedeemResult::CommandMismatch
        );
    }

    #[test]
    fn test_scope_isolation() {
        let s1 = store("proj-a");
        let code = s1.issue("git push --force");
        let s2 = store("proj-b");
        assert_eq!(s2.redeem(&code, "git push --force"), RedeemResult::NotFound);
    }

    #[test]
    fn test_expiry() {
        // 50ms TTL exercises the expiry path deterministically.
        let s = store("proj-a").with_ttl(Duration::from_millis(50));
        let code = s.issue("echo hi");
        assert_eq!(s.redeem(&code, "echo hi"), RedeemResult::Approved);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(s.redeem(&code, "echo hi"), RedeemResult::Expired);
    }

    #[test]
    fn test_not_found() {
        let s = store("proj-a");
        assert_eq!(s.redeem("zzzzz", "ls"), RedeemResult::NotFound);
    }

    #[test]
    fn test_same_command_distinct_codes() {
        let s = store("proj-a");
        let c1 = s.issue("ls -la");
        let c2 = s.issue("ls -la");
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_jsonl_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wf-allowonce-{}.jsonl", std::process::id()));
        let s = store("proj-a").with_jsonl(dir.clone());
        s.issue("git reset --hard");
        s.issue("docker system prune");
        s.persist(&dir).unwrap();

        let s2 = store("proj-a");
        let loaded = s2.load(&dir).unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(s2.pending_count(), 2);

        let _ = std::fs::remove_file(&dir);
    }
}
