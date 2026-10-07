use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use wf_types::config::file_checkpoint::FailureBehavior;

use crate::file::FileState;
use checkpoint_base::error::CheckpointError;

/// SHA-256 hex digest of a byte slice.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// Normalize a workspace root into the stable workspace key used to derive
/// workspace-scoped manual/staged partition ids: trailing path separators
/// are stripped, everything else is kept verbatim so the same root always
/// maps to the same key.
pub(crate) fn normalize_workspace_key(root: &Path) -> String {
    let raw = root.to_string_lossy();
    let trimmed = raw.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        raw.into_owned()
    } else {
        trimmed.to_string()
    }
}

/// Validate and normalize a workspace-relative file path.
pub(crate) fn validate_workspace_relative_path(path: &str) -> Result<String, CheckpointError> {
    if path.trim().is_empty() {
        return Err(CheckpointError::Validation {
            reason: "file path must not be empty".to_string(),
        });
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() || candidate.has_root() {
        return Err(CheckpointError::Validation {
            reason: format!("file path must be workspace-relative: '{path}'"),
        });
    }
    let mut depth = 0usize;
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if depth == 0 => {
                return Err(CheckpointError::Validation {
                    reason: format!("file path escapes workspace: '{path}'"),
                });
            }
            std::path::Component::ParentDir => {
                normalized.pop();
                depth -= 1;
            }
            std::path::Component::Normal(part) => {
                normalized.push(part);
                depth += 1;
            }
            _ => {
                return Err(CheckpointError::Validation {
                    reason: format!("invalid workspace file path: '{path}'"),
                });
            }
        }
    }
    if depth == 0 {
        return Err(CheckpointError::Validation {
            reason: format!("file path must name a file: '{path}'"),
        });
    }
    Ok(normalized.to_string_lossy().replace('\\', "/"))
}

/// SHA-256 of the sorted `path=hash;` pairs (stable workspace fingerprint).
pub(crate) fn compute_full_hash(files: &[FileState]) -> String {
    let mut parts: Vec<&FileState> = files.iter().collect();
    parts.sort_by(|a, b| a.path.cmp(&b.path));
    let mut hasher = Sha256::new();
    for f in parts {
        hasher.update(f.path.as_bytes());
        hasher.update(b"=");
        hasher.update(f.hash.as_bytes());
        hasher.update(b";");
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

pub(crate) fn write_file_with_dirs(target: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, content)
}

pub(crate) fn handle_restore_failure(
    behavior: FailureBehavior,
    path: &str,
    err: &std::io::Error,
) -> Result<(), CheckpointError> {
    match behavior {
        FailureBehavior::Error => Err(CheckpointError::Io(std::io::Error::other(format!(
            "failed to restore '{path}': {err}"
        )))),
        FailureBehavior::Warn => {
            tracing::warn!("failed to restore '{path}': {err}");
            Ok(())
        }
        FailureBehavior::Ignore => Ok(()),
    }
}

/// Resolve the on-disk target for a restored file state.
pub(crate) fn resolve_restore_target(
    base_dir: &Path,
    path: &str,
) -> Result<PathBuf, CheckpointError> {
    let relative = validate_workspace_relative_path(path)?;
    let joined = base_dir.join(relative);
    let base = base_dir.canonicalize().map_err(CheckpointError::Io)?;
    let mut existing = joined.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("cannot resolve restore path '{path}'"),
            })?;
    }
    let canonical_parent = existing.canonicalize().map_err(CheckpointError::Io)?;
    if !canonical_parent.starts_with(&base) {
        return Err(CheckpointError::Validation {
            reason: format!(
                "file checkpoint path '{}' escapes base directory '{}'",
                path,
                base_dir.display()
            ),
        });
    }
    Ok(joined)
}

/// Lexical normalization without touching the filesystem.
/// Fallback root actor for a bare execution id (agent kind).
pub(crate) fn root_actor(execution_id: wf_types::Id) -> checkpoint_base::actor::id::ActorId {
    checkpoint_base::actor::id::ActorId::new(
        checkpoint_base::actor::id::ActorKind::Agent,
        &[execution_id],
    )
    .unwrap_or_else(|_| {
        checkpoint_base::actor::id::ActorId::new(
            checkpoint_base::actor::id::ActorKind::Agent,
            &[wf_types::Id::from("unknown")],
        )
        .expect("invariant: the 'unknown' fallback actor id is always valid")
    })
}
