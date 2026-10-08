//! Independent bare Git object store for file-content history.
//!
//! Frozen rules (no behavior beyond this module may redefine them):
//! - one bare repository per workspace, stored at
//!   `<workspace>/.wf-checkpoint-git`, fully separated from the user's own
//!   repository (the user's `.git` is never read or written);
//! - refs live only under `refs/wf/`: `refs/wf/main` is the integration
//!   truth, `refs/wf/edit/<actor>` is one actor's edit line,
//!   `refs/wf/review/<id>` is one submission awaiting review,
//!   `refs/wf/feat/<name>` is one collaboration target,
//!   `refs/wf/human` carries external human edits and is never auto-merged;
//! - commit trailers carry attribution: `Wf-Actor`, `Wf-Session`, `Wf-Tool`;
//!   `Wf-State: conflict-unresolved` / `conflict-resolved` marks merge state;
//! - ignore stacking order: repository-local excludes (`.git`,
//!   `.wf-checkpoint-git`, checkpoint database file), then the workspace's
//!   own `.gitignore` files, then custom patterns;
//! - conflicts land on disk with standard `<<<<<<<` / `=======` / `>>>>>>>`
//!   markers and block the corresponding merge until resolved.
//!
//! Implementation notes: object storage, encoding and ref naming follow the
//! shared complexion via the Gitoxide split crates (no native toolchain,
//! offline deterministic). Writes are pipelined object operations that never
//! switch a worktree: trees are built from the parent tree plus input bytes,
//! merges run in memory, and the workspace files are only touched when
//! materializing a target state (serialized by the caller). Ref updates go
//! through Gitoxide transactions: the expectation is checked while holding
//! the ref lock, reflog writing is disabled (checkpoint refs never need
//! history), and stale lock files from crashed writers are reclaimed, so a
//! compare-and-swap failure always means the ref moved under us.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gix_object::{Find as _, Write as _};

/// Directory name of the bare repository inside the workspace root.
pub const CHECKPOINT_GIT_DIR_NAME: &str = ".wf-checkpoint-git";

/// Integration truth: the only ref whose tree may be materialized.
pub const REF_MAIN: &str = "refs/wf/main";
/// One actor's edit line: `refs/wf/edit/<actor>`.
pub const REF_EDIT_PREFIX: &str = "refs/wf/edit/";
/// One submission awaiting review: `refs/wf/review/<id>`.
pub const REF_REVIEW_PREFIX: &str = "refs/wf/review/";
/// One collaboration target: `refs/wf/feat/<name>`.
pub const REF_FEAT_PREFIX: &str = "refs/wf/feat/";
/// External human edits. Never auto-merged into any other ref.
pub const REF_HUMAN: &str = "refs/wf/human";

/// Commit trailer carrying the acting executor id.
pub const TRAILER_ACTOR: &str = "Wf-Actor";
/// Commit trailer carrying the session id shared by one operation group.
pub const TRAILER_SESSION: &str = "Wf-Session";
/// Commit trailer carrying the tool source of the operation.
pub const TRAILER_TOOL: &str = "Wf-Tool";
/// Commit trailer carrying merge state (`conflict-unresolved/resolved`).
pub const TRAILER_STATE: &str = "Wf-State";
/// Commit trailer value marking an unresolved merge.
pub const STATE_CONFLICT_UNRESOLVED: &str = "conflict-unresolved";
/// Commit trailer value marking a resolved merge.
pub const STATE_CONFLICT_RESOLVED: &str = "conflict-resolved";
/// Commit trailer repeating once per conflicted path.
pub const TRAILER_CONFLICT_FILE: &str = "Wf-Conflict-File";

/// Author/committer identity used for system-created commits.
pub const SYSTEM_COMMITTER: &str = "wf-checkpoint <system@local>";

/// Regular file mode used for every tracked blob.
pub const MODE_FILE: &str = "100644";
/// Executable file mode preserved from the worktree when present.
pub const MODE_EXEC: &str = "100755";

#[derive(Debug, thiserror::Error)]
pub enum GitStoreError {
    #[error("checkpoint git store not initialized: {0}")]
    Uninitialized(String),
    #[error("ref not found: {0}")]
    RefNotFound(String),
    #[error("object not found: {0}")]
    ObjectNotFound(String),
    #[error("corrupt object {id}: {reason}")]
    Corrupt { id: String, reason: String },
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("ref transaction conflict on '{0}'")]
    RefConflict(String),
    #[error("io error: {0}")]
    Io(String),
}

impl From<std::io::Error> for GitStoreError {
    fn from(e: std::io::Error) -> Self {
        GitStoreError::Io(e.to_string())
    }
}

/// One tree entry: mode, name and object id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: String,
    pub name: String,
    pub id: String,
}

/// Parsed commit object.
#[derive(Debug, Clone)]
pub struct GitCommit {
    pub id: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub committer: String,
    pub author_ts: i64,
    pub committer_ts: i64,
    pub message: String,
}

impl GitCommit {
    /// Value of the first trailer line with the given key, if any.
    pub fn trailer(&self, key: &str) -> Option<String> {
        trailers_of(&self.message)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// All values of a repeating trailer key, in order.
    pub fn trailers(&self, key: &str) -> Vec<String> {
        trailers_of(&self.message)
            .into_iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .collect()
    }
}

/// Parse `Key: value` trailers from the message tail.
pub fn trailers_of(message: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in message.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            if key.is_empty() || key.contains(' ') {
                break;
            }
            out.push((key.to_string(), v.trim().to_string()));
        } else {
            break;
        }
    }
    out.reverse();
    out
}

/// Whether a string is a well-formed object id (40 hex chars).
pub fn is_hex_id(id: &str) -> bool {
    id.len() == 40 && gix_hash::ObjectId::from_hex(id.as_bytes()).is_ok()
}

/// One merged file: bytes (`None` = deleted) plus conflict flag.
pub struct MergeFileOutcome {
    pub bytes: Option<Vec<u8>>,
    pub conflicted: bool,
}

/// Whether bytes look binary (NUL in the probed prefix).
pub fn is_binary_bytes(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// Standard three-way file merge over optional contents (`None` = the
/// file is absent on that side):
/// - all sides agree (including jointly absent) → that value, clean;
/// - one side changed relative to base → the changed side wins;
/// - added on exactly one side → the addition wins;
/// - overlapping changes (modify/delete, add/add, modify/modify) →
///   standard conflict markers, conflicted.
pub fn merge_file_contents(
    base: Option<&[u8]>,
    ours: Option<&[u8]>,
    theirs: Option<&[u8]>,
) -> MergeFileOutcome {
    if ours == theirs {
        return MergeFileOutcome {
            bytes: ours.map(|b| b.to_vec()),
            conflicted: false,
        };
    }
    if base == ours {
        return MergeFileOutcome {
            bytes: theirs.map(|b| b.to_vec()),
            conflicted: false,
        };
    }
    if base == theirs {
        return MergeFileOutcome {
            bytes: ours.map(|b| b.to_vec()),
            conflicted: false,
        };
    }
    // Overlapping changes: line-based text merge with standard markers
    // (absent sides read as empty).
    let ours_bytes = ours.unwrap_or(&[]);
    let theirs_bytes = theirs.unwrap_or(&[]);
    if is_binary_bytes(ours_bytes) || is_binary_bytes(theirs_bytes) {
        return MergeFileOutcome {
            bytes: Some(ours_bytes.to_vec()),
            conflicted: true,
        };
    }
    let base_bytes = base.unwrap_or(&[]);
    let mut merged = Vec::new();
    let mut input = gix_diff::blob::InternedInput::new(&b""[..], &b""[..]);
    let labels = gix_merge::blob::builtin_driver::text::Labels {
        ancestor: None,
        current: Some(gix_object::bstr::BStr::new("ours")),
        other: Some(gix_object::bstr::BStr::new("theirs")),
    };
    let resolution = gix_merge::blob::builtin_driver::text(
        &mut merged,
        &mut input,
        labels,
        ours_bytes,
        base_bytes,
        theirs_bytes,
        gix_merge::blob::builtin_driver::text::Options::default(),
    );
    MergeFileOutcome {
        bytes: Some(merged),
        conflicted: resolution == gix_merge::blob::Resolution::Conflict,
    }
}

/// Merge whole file maps with standard semantics. Returns the merged map
/// (`None` values are deletions) plus the sorted list of conflicted paths.
pub fn merge_file_maps(
    base: &HashMap<String, Vec<u8>>,
    ours: &HashMap<String, Vec<u8>>,
    theirs: &HashMap<String, Vec<u8>>,
) -> (HashMap<String, Option<Vec<u8>>>, Vec<String>) {
    let mut keys = HashSet::new();
    keys.extend(base.keys().cloned());
    keys.extend(ours.keys().cloned());
    keys.extend(theirs.keys().cloned());
    let mut merged = HashMap::new();
    let mut conflicts = Vec::new();
    for key in keys {
        let outcome = merge_file_contents(
            base.get(&key).map(|v| v.as_slice()),
            ours.get(&key).map(|v| v.as_slice()),
            theirs.get(&key).map(|v| v.as_slice()),
        );
        if outcome.conflicted {
            conflicts.push(key.clone());
        }
        merged.insert(key, outcome.bytes);
    }
    conflicts.sort();
    (merged, conflicts)
}

/// Edit ref for an actor id. Unsafe characters are sanitized so the ref path
/// stays inside `refs/wf/edit/`.
pub fn edit_ref_for_actor(actor: &str) -> String {
    format!("{REF_EDIT_PREFIX}{}", sanitize_ref_component(actor))
}

/// Review ref for a submission id.
pub fn review_ref_for_id(id: &str) -> String {
    format!("{REF_REVIEW_PREFIX}{}", sanitize_ref_component(id))
}

/// Feature ref for a collaboration target name.
pub fn feat_ref_for_name(name: &str) -> String {
    format!("{REF_FEAT_PREFIX}{}", sanitize_ref_component(name))
}

/// Ref domain only: maps arbitrary names into safe ref path segments.
/// Never use for workspace paths, which need relative validation or
/// absolute normalization instead.
fn sanitize_ref_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' || ch == '/' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches(|c| c == '/' || c == '.');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_object_id(id: &str) -> Result<gix_hash::ObjectId, GitStoreError> {
    if id.len() != 40 {
        return Err(GitStoreError::InvalidInput(format!("bad object id '{id}'")));
    }
    gix_hash::ObjectId::from_hex(id.as_bytes())
        .map_err(|e| GitStoreError::InvalidInput(format!("bad object id '{id}': {e}")))
}

/// Split a free-form actor string into a valid name/email pair. Inputs that
/// already carry an email keep it, bare names get a local placeholder.
fn split_actor(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed.find('>') {
            if start < end {
                let name = trimmed[..start].trim();
                let email = trimmed[start + 1..end].trim();
                if !email.is_empty() {
                    return (
                        if name.is_empty() {
                            "checkpoint".to_string()
                        } else {
                            name.to_string()
                        },
                        email.to_string(),
                    );
                }
            }
        }
    }
    if trimmed.is_empty() {
        ("checkpoint".to_string(), "local".to_string())
    } else {
        (trimmed.to_string(), "local".to_string())
    }
}

fn signature_for(raw: &str, secs: i64) -> gix_actor::Signature {
    let (name, email) = split_actor(raw);
    gix_actor::Signature {
        name: name.as_bytes().to_vec().into(),
        email: email.as_bytes().to_vec().into(),
        time: gix_date::Time::new(secs, 0),
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Independent bare repository handle for one workspace.
pub struct GitStore {
    git_dir: PathBuf,
    workspace_root: PathBuf,
    /// Owned temp dir for in-memory managers; `None` for real workspaces.
    /// The path is removed best-effort on drop.
    owned_temp: Option<PathBuf>,
}

impl Drop for GitStore {
    fn drop(&mut self) {
        if let Some(dir) = self.owned_temp.take() {
            if dir.starts_with(std::env::temp_dir()) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }
}

impl GitStore {
    /// Default bare repository location for a workspace root.
    pub fn default_git_dir(workspace_root: &Path) -> PathBuf {
        workspace_root.join(CHECKPOINT_GIT_DIR_NAME)
    }

    /// Open (creating if needed) the bare repository at `git_dir`.
    pub fn init(workspace_root: &Path, git_dir: &Path) -> Result<Self, GitStoreError> {
        fs::create_dir_all(git_dir.join("objects"))?;
        fs::create_dir_all(git_dir.join("refs/wf"))?;
        let head = git_dir.join("HEAD");
        if !head.exists() {
            fs::write(&head, format!("ref: {REF_MAIN}\n"))?;
        }
        let config = git_dir.join("config");
        if !config.exists() {
            fs::write(
                &config,
                "[core]\n\trepositoryformatversion = 0\n\tbare = true\n",
            )?;
        }
        Ok(Self {
            git_dir: git_dir.to_path_buf(),
            workspace_root: workspace_root.to_path_buf(),
            owned_temp: None,
        })
    }

    /// Open the default bare repository location for a workspace root.
    pub fn init_for_workspace(workspace_root: &Path) -> Result<Self, GitStoreError> {
        let git_dir = Self::default_git_dir(workspace_root);
        Self::init(workspace_root, &git_dir)
    }

    /// Ephemeral store under the system temp dir (tests / in-memory use).
    pub fn init_temp() -> Result<Self, GitStoreError> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("wf-checkpoint-git-{}-{}", std::process::id(), id));
        let workspace = dir.join("work");
        fs::create_dir_all(&workspace)?;
        let mut store = Self::init(&workspace, &dir.join("repo"))?;
        store.owned_temp = Some(dir);
        Ok(store)
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    // ── refs (Gitoxide transactions, reflog disabled) ──

    fn ref_path(&self, name: &str) -> Result<PathBuf, GitStoreError> {
        if name.is_empty()
            || name.starts_with('/')
            || name.contains("..")
            || !name.starts_with("refs/wf/")
        {
            return Err(GitStoreError::InvalidInput(format!(
                "ref must stay under refs/wf/: '{name}'"
            )));
        }
        Ok(self.git_dir.join(name))
    }

    fn ref_full_name(&self, name: &str) -> Result<gix_ref::FullName, GitStoreError> {
        self.ref_path(name)?;
        gix_ref::FullName::try_from(name)
            .map_err(|e| GitStoreError::InvalidInput(format!("invalid ref name '{name}': {e}")))
    }

    fn ref_target(id: &str) -> Result<gix_ref::Target, GitStoreError> {
        parse_object_id(id).map(gix_ref::Target::Object)
    }

    fn ref_store(&self) -> gix_ref::file::Store {
        gix_ref::file::Store::at_opts(
            self.git_dir.clone(),
            gix_hash::Kind::Sha1,
            gix_ref::store::init::Options {
                write_reflog: gix_ref::store::WriteReflog::Disable,
                ..Default::default()
            },
        )
    }

    /// Drop a lock file left behind by a crashed writer. Only locks older
    /// than a minute are removed: live transactions finish in milliseconds,
    /// so an old lock can only be stale. Best effort, never fails.
    fn clear_stale_ref_lock(&self, name: &str) {
        let Ok(path) = self.ref_path(name) else {
            return;
        };
        let mut lock = path;
        lock.as_mut_os_string().push(".lock");
        let stale = fs::symlink_metadata(&lock)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(60));
        if stale {
            let _ = fs::remove_file(&lock);
        }
    }

    fn commit_ref_edit(
        &self,
        name: &str,
        edit: gix_ref::transaction::RefEdit,
        fail: gix_lock::acquire::Fail,
    ) -> Result<(), GitStoreError> {
        self.clear_stale_ref_lock(name);
        let committer: Option<gix_actor::SignatureRef<'_>> = None;
        self.ref_store()
            .transaction()
            .prepare([edit], fail, fail)
            .and_then(|transaction| transaction.commit(committer))
            .map(|_| ())
            .map_err(|e| GitStoreError::Io(e.to_string()))
    }

    /// Read a ref. `Ok(None)` means the ref does not exist yet.
    pub fn read_ref(&self, name: &str) -> Result<Option<String>, GitStoreError> {
        let path = self.ref_path(name)?;
        match fs::read_to_string(&path) {
            Ok(content) => {
                let id = content.trim().to_string();
                if id.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(id))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(GitStoreError::from(e)),
        }
    }

    /// Atomically point a ref at `id`. Concurrent writers to the same ref
    /// are serialized; writers to different refs never block each other.
    pub fn write_ref(&self, name: &str, id: &str) -> Result<(), GitStoreError> {
        let full_name = self.ref_full_name(name)?;
        let edit = gix_ref::transaction::RefEdit::update(
            full_name,
            Self::ref_target(id)?,
            gix_ref::transaction::PreviousValue::Any,
            "",
        );
        self.commit_ref_edit(
            name,
            edit,
            gix_lock::acquire::Fail::AfterDurationWithBackoff(Duration::from_millis(500)),
        )
    }

    /// Atomic compare-and-swap: only update when the current value equals
    /// `expected` (`None` = must not exist). The expectation is checked
    /// while holding the ref lock, so a concurrent writer cannot slip
    /// between the check and the update.
    pub fn compare_and_swap(
        &self,
        name: &str,
        expected: Option<&str>,
        next: &str,
    ) -> Result<(), GitStoreError> {
        let full_name = self.ref_full_name(name)?;
        let expected_value = match expected {
            None => gix_ref::transaction::PreviousValue::MustNotExist,
            Some(want) => {
                gix_ref::transaction::PreviousValue::MustExistAndMatch(Self::ref_target(want)?)
            }
        };
        let edit = gix_ref::transaction::RefEdit::update(
            full_name,
            Self::ref_target(next)?,
            expected_value,
            "",
        );
        if let Err(error) = self.commit_ref_edit(name, edit, gix_lock::acquire::Fail::Immediately) {
            let current = self.read_ref(name)?;
            if current.as_deref() != expected {
                return Err(GitStoreError::RefConflict(name.to_string()));
            }
            return Err(GitStoreError::Io(error.to_string()));
        }
        Ok(())
    }

    /// Delete a ref. Missing refs are a no-op success.
    pub fn delete_ref(&self, name: &str) -> Result<(), GitStoreError> {
        let full_name = self.ref_full_name(name)?;
        let edit = gix_ref::transaction::RefEdit::delete(
            full_name,
            gix_ref::transaction::PreviousValue::Any,
        );
        self.commit_ref_edit(
            name,
            edit,
            gix_lock::acquire::Fail::AfterDurationWithBackoff(Duration::from_millis(500)),
        )
    }

    /// Point `dst` at the same commit as `src` without merging.
    pub fn copy_ref(&self, src: &str, dst: &str) -> Result<String, GitStoreError> {
        let id = self
            .read_ref(src)?
            .ok_or_else(|| GitStoreError::RefNotFound(src.to_string()))?;
        self.write_ref(dst, &id)?;
        Ok(id)
    }

    /// List `(refname, id)` pairs under a prefix, sorted by refname. A
    /// prefix that names a ref exactly matches that single ref. Only
    /// object targets are listed: a symbolic link under `refs/wf/` would
    /// violate the store rules and is reported instead of being returned
    /// as an id.
    pub fn list_refs(&self, prefix: &str) -> Result<Vec<(String, String)>, GitStoreError> {
        let mut out = Vec::new();
        let store = self.ref_store();
        let platform = store.iter().map_err(|e| GitStoreError::Io(e.to_string()))?;
        let refs = platform.all()?;
        for reference in refs {
            let reference = reference.map_err(|e| GitStoreError::Io(e.to_string()))?;
            let name = reference.name.to_string();
            if !name.starts_with(prefix) {
                continue;
            }
            match reference.target {
                gix_ref::Target::Object(id) => out.push((name, id.to_hex().to_string())),
                gix_ref::Target::Symbolic(_) => {
                    return Err(GitStoreError::InvalidInput(format!(
                        "ref '{name}' must point at an object id"
                    )));
                }
            }
        }
        Ok(out)
    }

    // ── objects (Gitoxide loose store: compression and identity handled there) ──

    fn odb(&self) -> Result<gix_odb::Handle, GitStoreError> {
        gix_odb::at(self.git_dir.join("objects"), gix_hash::Kind::Sha1)
            .map_err(|e| GitStoreError::Io(e.to_string()))
    }

    fn find_data(&self, id: &str) -> Result<(gix_object::Kind, Vec<u8>), GitStoreError> {
        let oid = parse_object_id(id)?;
        let odb = self.odb()?;
        let mut buf = Vec::new();
        let data = odb
            .try_find(&oid, &mut buf)
            .map_err(|e| GitStoreError::Corrupt {
                id: id.to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| GitStoreError::ObjectNotFound(id.to_string()))?;
        Ok((data.kind, data.data.to_vec()))
    }

    /// Store bytes as a blob object. Returns the object id.
    pub fn write_blob(&self, bytes: &[u8]) -> Result<String, GitStoreError> {
        let oid = self
            .odb()?
            .write_buf(gix_object::Kind::Blob, bytes)
            .map_err(|e| GitStoreError::Corrupt {
                id: "<new-blob>".to_string(),
                reason: e.to_string(),
            })?;
        Ok(oid.to_hex().to_string())
    }

    /// Load a blob's bytes.
    pub fn read_blob(&self, id: &str) -> Result<Vec<u8>, GitStoreError> {
        let (kind, body) = self.find_data(id)?;
        if kind != gix_object::Kind::Blob {
            return Err(GitStoreError::Corrupt {
                id: id.to_string(),
                reason: format!("expected blob, found {kind}"),
            });
        }
        Ok(body)
    }

    /// Store a sorted tree object from `(mode, name, id)` entries.
    pub fn write_tree(&self, entries: &[TreeEntry]) -> Result<String, GitStoreError> {
        let mut sorted = entries.to_vec();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        let mut tree_entries = Vec::with_capacity(sorted.len());
        for entry in &sorted {
            let kind = match entry.mode.as_str() {
                "40000" => gix_object::tree::EntryKind::Tree,
                MODE_EXEC => gix_object::tree::EntryKind::BlobExecutable,
                MODE_FILE => gix_object::tree::EntryKind::Blob,
                other => {
                    return Err(GitStoreError::InvalidInput(format!(
                        "unsupported tree entry mode '{other}'"
                    )));
                }
            };
            let oid = parse_object_id(&entry.id)?;
            tree_entries.push(gix_object::tree::Entry {
                mode: kind.into(),
                filename: entry.name.as_bytes().to_vec().into(),
                oid,
            });
        }
        let tree = gix_object::Tree {
            entries: tree_entries,
        };
        let oid = self
            .odb()?
            .write(&tree)
            .map_err(|e| GitStoreError::Corrupt {
                id: "<new-tree>".to_string(),
                reason: e.to_string(),
            })?;
        Ok(oid.to_hex().to_string())
    }

    /// Parse a tree object into entries.
    pub fn read_tree(&self, id: &str) -> Result<Vec<TreeEntry>, GitStoreError> {
        let (kind, body) = self.find_data(id)?;
        if kind != gix_object::Kind::Tree {
            return Err(GitStoreError::Corrupt {
                id: id.to_string(),
                reason: format!("expected tree, found {kind}"),
            });
        }
        let tree = gix_object::TreeRef::from_bytes(&body, gix_hash::Kind::Sha1).map_err(|e| {
            GitStoreError::Corrupt {
                id: id.to_string(),
                reason: e.to_string(),
            }
        })?;
        Ok(tree
            .entries
            .iter()
            .map(|e| TreeEntry {
                mode: format!("{:o}", e.mode),
                name: e.filename.to_string(),
                id: e.oid.to_hex().to_string(),
            })
            .collect())
    }

    /// Expand a tree recursively into `path -> (mode, blob id)`.
    pub fn tree_to_files(
        &self,
        tree_id: &str,
    ) -> Result<HashMap<String, (String, String)>, GitStoreError> {
        let mut out = HashMap::new();
        self.tree_to_files_into(tree_id, String::new(), &mut out)?;
        Ok(out)
    }

    fn tree_to_files_into(
        &self,
        tree_id: &str,
        prefix: String,
        out: &mut HashMap<String, (String, String)>,
    ) -> Result<(), GitStoreError> {
        for entry in self.read_tree(tree_id)? {
            let path = if prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", prefix, entry.name)
            };
            if entry.mode == "40000" {
                self.tree_to_files_into(&entry.id, path, out)?;
            } else {
                out.insert(path, (entry.mode, entry.id));
            }
        }
        Ok(())
    }

    /// Load a tree recursively into `path -> bytes`.
    pub fn tree_to_bytes(&self, tree_id: &str) -> Result<HashMap<String, Vec<u8>>, GitStoreError> {
        let files = self.tree_to_files(tree_id)?;
        let mut out = HashMap::with_capacity(files.len());
        for (path, (_, blob)) in files {
            out.insert(path, self.read_blob(&blob)?);
        }
        Ok(out)
    }

    /// Build a new tree from a parent tree plus path changes
    /// (`None` blob = deletion). Pure object pipelining, no checkout.
    pub fn build_tree_from_parent(
        &self,
        parent_tree: Option<&str>,
        changes: &HashMap<String, Option<(String, Vec<u8>)>>,
    ) -> Result<String, GitStoreError> {
        // Split every tree level into nested maps, apply the changes at the
        // leaves, then rebuild bottom-up.
        #[derive(Default)]
        struct Node {
            blobs: HashMap<String, (String, String)>,
            trees: HashMap<String, Box<Node>>,
        }
        fn load(store: &GitStore, tree_id: &str, node: &mut Node) -> Result<(), GitStoreError> {
            for entry in store.read_tree(tree_id)? {
                if entry.mode == "40000" {
                    let mut child = Box::new(Node::default());
                    load(store, &entry.id, &mut child)?;
                    node.trees.insert(entry.name, child);
                } else {
                    node.blobs.insert(entry.name, (entry.mode, entry.id));
                }
            }
            Ok(())
        }
        let mut root = Node::default();
        if let Some(tree) = parent_tree {
            load(self, tree, &mut root)?;
        }
        for (path, change) in changes {
            let mut parts: Vec<&str> = path.split('/').collect();
            let Some(leaf) = parts.pop() else { continue };
            let mut node = &mut root;
            let mut ok = true;
            for part in parts {
                if node.blobs.contains_key(part) {
                    ok = false;
                    break;
                }
                node = node
                    .trees
                    .entry(part.to_string())
                    .or_insert_with(|| Box::new(Node::default()));
            }
            if !ok {
                return Err(GitStoreError::InvalidInput(format!(
                    "path collides with a file: '{path}'"
                )));
            }
            match change {
                Some((mode, bytes)) => {
                    let blob = self.write_blob(bytes)?;
                    node.blobs.insert(leaf.to_string(), (mode.clone(), blob));
                    node.trees.remove(leaf);
                }
                None => {
                    node.blobs.remove(leaf);
                    node.trees.remove(leaf);
                }
            }
        }
        fn store_node(git: &GitStore, node: &Node) -> Result<String, GitStoreError> {
            let mut entries = Vec::new();
            for (name, child) in &node.trees {
                if child.blobs.is_empty() && child.trees.is_empty() {
                    continue;
                }
                entries.push(TreeEntry {
                    mode: "40000".to_string(),
                    name: name.clone(),
                    id: store_node(git, child)?,
                });
            }
            for (name, (mode, blob)) in &node.blobs {
                entries.push(TreeEntry {
                    mode: mode.clone(),
                    name: name.clone(),
                    id: blob.clone(),
                });
            }
            git.write_tree(&entries)
        }
        store_node(self, &root)
    }

    /// Store a commit object. Empty-change commits are the caller's
    /// responsibility to skip (compare trees first).
    pub fn write_commit(
        &self,
        tree: &str,
        parents: &[String],
        author: &str,
        committer: &str,
        timestamp_millis: i64,
        message: &str,
    ) -> Result<String, GitStoreError> {
        let tree_id = parse_object_id(tree)?;
        let mut parent_ids = Vec::with_capacity(parents.len());
        for parent in parents {
            parent_ids.push(parse_object_id(parent)?);
        }
        let secs = timestamp_millis.div_euclid(1000);
        let mut text = message.trim_end_matches('\n').to_string();
        text.push('\n');
        let commit = gix_object::Commit {
            tree: tree_id,
            parents: parent_ids.into_iter().collect(),
            author: signature_for(author, secs),
            committer: signature_for(committer, secs),
            encoding: None,
            message: text.as_bytes().to_vec().into(),
            extra_headers: Vec::new(),
        };
        let oid = self
            .odb()?
            .write(&commit)
            .map_err(|e| GitStoreError::Corrupt {
                id: "<new-commit>".to_string(),
                reason: e.to_string(),
            })?;
        Ok(oid.to_hex().to_string())
    }

    /// Parse a commit object.
    pub fn read_commit(&self, id: &str) -> Result<GitCommit, GitStoreError> {
        let (kind, body) = self.find_data(id)?;
        if kind != gix_object::Kind::Commit {
            return Err(GitStoreError::Corrupt {
                id: id.to_string(),
                reason: format!("expected commit, found {kind}"),
            });
        }
        let commit =
            gix_object::CommitRef::from_bytes(&body, gix_hash::Kind::Sha1).map_err(|e| {
                GitStoreError::Corrupt {
                    id: id.to_string(),
                    reason: e.to_string(),
                }
            })?;
        let author_sig = commit.author().map_err(|e| GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        })?;
        let committer_sig = commit.committer().map_err(|e| GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        })?;
        Ok(GitCommit {
            id: id.to_string(),
            tree: commit.tree().to_hex().to_string(),
            parents: commit.parents().map(|o| o.to_hex().to_string()).collect(),
            author: author_sig.name.to_string(),
            committer: committer_sig.name.to_string(),
            author_ts: author_sig.seconds().saturating_mul(1000),
            committer_ts: committer_sig.seconds().saturating_mul(1000),
            message: commit
                .message
                .to_string()
                .trim_end_matches('\n')
                .to_string(),
        })
    }

    /// Append a commit on a ref: build the tree from the ref head's tree
    /// plus `changes`, skip creating a commit when nothing changed (returns
    /// the existing head id), otherwise write the commit and move the ref.
    /// One call = one commit; multi-file operations pass all files at once
    /// so they stay atomic.
    pub fn commit_on_ref(
        &self,
        refname: &str,
        changes: &HashMap<String, Option<(String, Vec<u8>)>>,
        author: &str,
        message: &str,
    ) -> Result<CommitOnRefOutcome, GitStoreError> {
        let head = self.read_ref(refname)?;
        let parent_tree = match &head {
            Some(id) => Some(self.read_commit(id)?.tree),
            None => None,
        };
        let tree = self.build_tree_from_parent(parent_tree.as_deref(), changes)?;
        if let Some(parent_tree) = parent_tree.as_deref() {
            if parent_tree == tree {
                return Ok(CommitOnRefOutcome {
                    id: head.unwrap_or_default(),
                    created: false,
                });
            }
        }
        let parents = head.into_iter().collect::<Vec<_>>();
        let id = self.write_commit(
            &tree,
            &parents,
            author,
            SYSTEM_COMMITTER,
            now_millis(),
            message,
        )?;
        self.write_ref(refname, &id)?;
        Ok(CommitOnRefOutcome { id, created: true })
    }

    /// Walk the commit graph from `start`, newest first, up to `limit`
    /// commits (0 = unlimited).
    pub fn log(&self, start: &str, limit: usize) -> Result<Vec<GitCommit>, GitStoreError> {
        let tip = parse_object_id(start)?;
        let odb = self.odb()?;
        let mut out = Vec::new();
        let walk = gix_traverse::commit::Simple::new([tip], odb);
        for next in walk {
            if limit > 0 && out.len() >= limit {
                break;
            }
            let Ok(info) = next else { continue };
            let id = info.id.to_hex().to_string();
            let Ok(commit) = self.read_commit(&id) else {
                continue;
            };
            out.push(commit);
        }
        out.sort_by(|a, b| b.committer_ts.cmp(&a.committer_ts).then(b.id.cmp(&a.id)));
        Ok(out)
    }

    /// All commits reachable from any ref (used for cold index rebuilds).
    pub fn all_commits(&self) -> Result<Vec<GitCommit>, GitStoreError> {
        let refs = self.list_refs("refs/wf/")?;
        let mut tips = Vec::with_capacity(refs.len());
        for (_, id) in &refs {
            if let Ok(oid) = parse_object_id(id) {
                tips.push(oid);
            }
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        if tips.is_empty() {
            return Ok(out);
        }
        let odb = self.odb()?;
        let walk = gix_traverse::commit::Simple::new(tips, odb);
        for next in walk {
            let Ok(info) = next else { continue };
            let id = info.id.to_hex().to_string();
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(commit) = self.read_commit(&id) else {
                continue;
            };
            out.push(commit);
        }
        Ok(out)
    }

    /// Whether `ancestor` is reachable from `descendant`.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, GitStoreError> {
        let ancestor_oid = parse_object_id(ancestor)?;
        let descendant_oid = parse_object_id(descendant)?;
        if ancestor_oid == descendant_oid {
            return Ok(true);
        }
        let odb = self.odb()?;
        let walk = gix_traverse::commit::Simple::new([descendant_oid], odb);
        for next in walk {
            let Ok(info) = next else { continue };
            if info.id == ancestor_oid {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Best common ancestor of two commits, if any.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>, GitStoreError> {
        let a_oid = parse_object_id(a)?;
        let b_oid = parse_object_id(b)?;
        if a_oid == b_oid {
            return Ok(Some(a.to_string()));
        }
        let mut ancestors = HashSet::new();
        {
            let odb = self.odb()?;
            let walk = gix_traverse::commit::Simple::new([a_oid], odb);
            for next in walk {
                let Ok(info) = next else { continue };
                ancestors.insert(info.id);
            }
        }
        {
            let odb = self.odb()?;
            let walk = gix_traverse::commit::Simple::new([b_oid], odb);
            for next in walk {
                let Ok(info) = next else { continue };
                if ancestors.contains(&info.id) {
                    return Ok(Some(info.id.to_hex().to_string()));
                }
            }
        }
        Ok(None)
    }

    /// Head commits of review/feature refs whose message still carries the
    /// unresolved marker. Each entry is `(refname, commit id, files)`.
    pub fn list_unresolved_conflicts(
        &self,
    ) -> Result<Vec<(String, String, Vec<String>)>, GitStoreError> {
        let mut out = Vec::new();
        for (name, id) in self
            .list_refs(REF_REVIEW_PREFIX)?
            .into_iter()
            .chain(self.list_refs(REF_FEAT_PREFIX)?)
            .chain(self.list_refs(REF_MAIN)?)
        {
            let Ok(commit) = self.read_commit(&id) else {
                continue;
            };
            if commit.trailer(TRAILER_STATE).as_deref() == Some(STATE_CONFLICT_UNRESOLVED) {
                out.push((name, id, commit.trailers(TRAILER_CONFLICT_FILE)));
            }
        }
        Ok(out)
    }
}

/// Result of [`GitStore::commit_on_ref`].
pub struct CommitOnRefOutcome {
    pub id: String,
    pub created: bool,
}

/// Build a commit message from an intent line plus trailers.
pub fn commit_message(
    intent: &str,
    actor: Option<&str>,
    session: Option<&str>,
    tool: Option<&str>,
    extra_trailers: &[(String, String)],
) -> String {
    let mut message = intent.trim().to_string();
    if message.is_empty() {
        message = "checkpoint".to_string();
    }
    if let Some(actor) = actor {
        message.push_str(&format!("\n{TRAILER_ACTOR}: {actor}"));
    }
    if let Some(session) = session.filter(|s| !s.is_empty()) {
        message.push_str(&format!("\n{TRAILER_SESSION}: {session}"));
    }
    if let Some(tool) = tool.filter(|t| !t.is_empty()) {
        message.push_str(&format!("\n{TRAILER_TOOL}: {tool}"));
    }
    for (k, v) in extra_trailers {
        message.push_str(&format!("\n{k}: {v}"));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> GitStore {
        GitStore::init_temp().expect("temp git store")
    }

    fn changes(pairs: &[(&str, &[u8])]) -> HashMap<String, Option<(String, Vec<u8>)>> {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), Some((MODE_FILE.to_string(), b.to_vec()))))
            .collect()
    }

    #[test]
    fn refs_roundtrip_atomically() {
        let store = store();
        assert_eq!(store.read_ref(REF_MAIN).unwrap(), None);
        let tree = store.write_tree(&[]).unwrap();
        let id = store
            .write_commit(&tree, &[], "a", SYSTEM_COMMITTER, 1_000, "init")
            .unwrap();
        store.write_ref(REF_MAIN, &id).unwrap();
        assert_eq!(
            store.read_ref(REF_MAIN).unwrap().as_deref(),
            Some(id.as_str())
        );
        assert!(store.compare_and_swap(REF_MAIN, Some("nope"), &id).is_err());
        store.compare_and_swap(REF_MAIN, Some(&id), &id).unwrap();
        assert_eq!(store.copy_ref(REF_MAIN, REF_HUMAN).unwrap(), id);
        // Ref transactions write no reflog sidecar files.
        assert!(!store.git_dir().join("logs").exists());
        // Prefix and exact-name listings agree.
        let refs = store.list_refs("refs/wf/").unwrap();
        assert!(refs.contains(&(REF_MAIN.to_string(), id.clone())));
        assert!(refs.contains(&(REF_HUMAN.to_string(), id.clone())));
        assert_eq!(
            store.list_refs(REF_MAIN).unwrap(),
            vec![(REF_MAIN.to_string(), id.clone())]
        );
        // Deleting a missing ref is a no-op success.
        store.delete_ref("refs/wf/review/missing").unwrap();
    }

    #[test]
    fn concurrent_compare_and_swap_keeps_one_winner() {
        let store = store();
        let tree = store.write_tree(&[]).unwrap();
        let first = store
            .write_commit(&tree, &[], "a", SYSTEM_COMMITTER, 1_000, "one")
            .unwrap();
        let second = store
            .write_commit(&tree, &[], "a", SYSTEM_COMMITTER, 2_000, "two")
            .unwrap();
        store.write_ref(REF_MAIN, &first).unwrap();
        std::thread::scope(|scope| {
            let a = scope.spawn(|| store.compare_and_swap(REF_MAIN, Some(&first), &second));
            let b = scope.spawn(|| store.compare_and_swap(REF_MAIN, Some(&first), &second));
            let results = [a.join().unwrap(), b.join().unwrap()];
            assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        });
        assert_eq!(
            store.read_ref(REF_MAIN).unwrap().as_deref(),
            Some(second.as_str())
        );
    }

    #[test]
    fn linear_commits_skip_empty() {
        let store = store();
        let first = store
            .commit_on_ref(
                &edit_ref_for_actor("agent:x"),
                &changes(&[("a.txt", b"hi")]),
                "agent:x",
                "edit",
            )
            .unwrap();
        assert!(first.created);
        let second = store
            .commit_on_ref(
                &edit_ref_for_actor("agent:x"),
                &changes(&[("a.txt", b"hi")]),
                "agent:x",
                "edit",
            )
            .unwrap();
        assert!(!second.created);
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn trees_roundtrip_nested_paths() {
        let store = store();
        let tree = store
            .build_tree_from_parent(None, &changes(&[("sub/a.txt", b"a"), ("b.txt", b"b")]))
            .unwrap();
        let files = store.tree_to_files(&tree).unwrap();
        assert_eq!(files.len(), 2);
        let bytes = store.tree_to_bytes(&tree).unwrap();
        assert_eq!(bytes["sub/a.txt"], b"a");
    }

    #[test]
    fn merge_semantics_are_standard() {
        let base: HashMap<String, Vec<u8>> = [("f".to_string(), b"line\n".to_vec())]
            .into_iter()
            .collect();
        let mut ours = base.clone();
        ours.insert("f".to_string(), b"ours\n".to_vec());
        let (merged, conflicts) = merge_file_maps(&base, &ours, &base);
        assert!(conflicts.is_empty());
        assert_eq!(merged["f"].clone().unwrap(), b"ours\n");
        let mut theirs = base.clone();
        theirs.insert("f".to_string(), b"theirs\n".to_vec());
        let (merged, conflicts) = merge_file_maps(&base, &ours, &theirs);
        assert_eq!(conflicts, vec!["f".to_string()]);
        let text = String::from_utf8(merged["f"].clone().unwrap()).unwrap();
        assert!(text.contains("<<<<<<< ours"));
        assert!(text.contains("======="));
        assert!(text.contains(">>>>>>> theirs"));

        // Added on one side only merges cleanly.
        let empty: HashMap<String, Vec<u8>> = HashMap::new();
        let (merged, conflicts) = merge_file_maps(&empty, &empty, &theirs);
        assert!(conflicts.is_empty());
        assert_eq!(merged["f"].clone().unwrap(), b"theirs\n");

        // Deleted on both sides stays deleted without conflict.
        let (merged, conflicts) = merge_file_maps(&base, &empty, &empty);
        assert!(conflicts.is_empty());
        assert_eq!(merged["f"], None);
    }

    #[test]
    fn ancestor_and_merge_base() {
        let store = store();
        let a = store
            .commit_on_ref(
                &edit_ref_for_actor("a"),
                &changes(&[("f", b"1")]),
                "a",
                "one",
            )
            .unwrap()
            .id;
        let b = store
            .commit_on_ref(
                &edit_ref_for_actor("a"),
                &changes(&[("f", b"2")]),
                "a",
                "two",
            )
            .unwrap()
            .id;
        assert!(store.is_ancestor(&a, &b).unwrap());
        assert!(!store.is_ancestor(&b, &a).unwrap());
        assert_eq!(
            store.merge_base(&a, &b).unwrap().as_deref(),
            Some(a.as_str())
        );
    }
}
