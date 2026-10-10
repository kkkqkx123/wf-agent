use crate::git_store::objects::parse_object_id;
use crate::git_store::{
    GitStore, GitStoreError, REF_EDIT_PREFIX, REF_FEAT_PREFIX, REF_REVIEW_PREFIX,
};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

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
/// absolute normalization instead. This is the single ref sanitizer;
/// submission-id shaping reuses it so ref naming never diverges.
pub(crate) fn sanitize_ref_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' || ch == '/' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let mut cleaned = out.replace("..", "__");
    while cleaned.contains("//") {
        cleaned = cleaned.replace("//", "/_");
    }
    let trimmed = cleaned.trim_matches(|c| c == '/' || c == '.').to_string();
    if trimmed.is_empty() {
        return "unnamed".to_string();
    }
    if trimmed.ends_with(".lock") {
        return format!("{trimmed}_");
    }
    trimmed
}

impl GitStore {
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
        let metadata = match fs::symlink_metadata(&lock) {
            Ok(meta) => meta,
            Err(_) => return,
        };
        let stale = match metadata.modified() {
            Ok(modified) => match modified.elapsed() {
                Ok(age) => age > Duration::from_secs(60),
                Err(_) => true,
            },
            Err(_) => true,
        };
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
        for attempt in 0..3u32 {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    let id = content.trim().to_string();
                    if id.is_empty() {
                        return Ok(None);
                    } else {
                        return Ok(Some(id));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if attempt < 2 {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    return Ok(None);
                }
                Err(e) => return Err(GitStoreError::from(e)),
            }
        }
        Ok(None)
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
        if let Err(error) = self.commit_ref_edit(
            name,
            edit,
            gix_lock::acquire::Fail::AfterDurationWithBackoff(Duration::from_millis(500)),
        ) {
            let current = self.read_ref(name)?;
            if current.as_deref() != expected {
                return Err(GitStoreError::RefConflict(name.to_string()));
            }
            let message = error.to_string();
            if message.contains("must exist")
                || message.contains("mismatch")
                || message.contains("conflict")
            {
                return Err(GitStoreError::RefConflict(name.to_string()));
            }
            return Err(GitStoreError::Io(message));
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
    /// object targets are listed; symbolic refs under the prefix are
    /// skipped with a warning so one stray link never breaks listings.
    pub fn list_refs(&self, prefix: &str) -> Result<Vec<(String, String)>, GitStoreError> {
        let mut out = Vec::new();
        let store = self.ref_store();
        let platform = store.iter().map_err(|e| GitStoreError::Io(e.to_string()))?;
        let refs = platform
            .all()
            .map_err(|e| GitStoreError::Io(e.to_string()))?;
        for reference in refs {
            let reference = reference.map_err(|e| GitStoreError::Io(e.to_string()))?;
            let name = reference.name.to_string();
            if !name.starts_with(prefix) {
                continue;
            }
            match reference.target {
                gix_ref::Target::Object(id) => out.push((name, id.to_hex().to_string())),
                gix_ref::Target::Symbolic(_) => {
                    tracing::warn!(refname = %name, "skipping symbolic ref under managed prefix");
                    continue;
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
}
