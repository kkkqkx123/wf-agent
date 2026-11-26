//! Path-level approval grants resolved from authorization presets.
//!
//! Grants relax only the human-approval decision. Execution scope, checkpoint
//! tracking and sandbox policy stay authoritative and are never widened here.

use wf_types::config::file_scope::scope_path_contains;
use wf_types::tool::file_permission::FileOperationType;
use wf_types::tool::path_grant::PathGrant;

use super::file_policy::normalize_file_path;

const PERMANENT_DENY: &[&str] = &[
    "**/credentials.json",
    "**/secrets/**",
    "**/*.pem",
    "**/*.key",
];

fn matches_pattern(pattern: &str, path: &str) -> bool {
    if !pattern.contains('*') && !pattern.contains('?') {
        return scope_path_contains(pattern.trim_end_matches('/'), path);
    }
    let Ok(glob) = globset::GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(true)
        .build()
    else {
        return false;
    };
    let matcher = glob.compile_matcher();
    matcher.is_match(path)
}

/// Secrets, credentials and keys stay outside any path grant. Environment
/// files are intentionally not in this floor: they are configuration that a
/// user may explicitly authorize for a session.
pub fn is_permanently_denied(path: &str) -> bool {
    let normalized = normalize_file_path(path);
    PERMANENT_DENY
        .iter()
        .any(|pattern| matches_pattern(pattern, &normalized))
}

fn operation_matches(grant: &PathGrant, operation: &FileOperationType) -> bool {
    match operation {
        FileOperationType::Read => grant.operation.allows_read(),
        FileOperationType::Write | FileOperationType::Delete => grant.operation.allows_write(),
    }
}

/// True when every path is covered by a grant with the requested operation
/// and none of them is on the permanent-deny floor. An empty path list is
/// never granted: callers fall through to the normal policy.
pub fn granted(
    grants: Option<&[PathGrant]>,
    paths: &[String],
    operation: &FileOperationType,
) -> bool {
    let Some(grants) = grants else {
        return false;
    };
    if paths.is_empty() || paths.iter().any(|path| is_permanently_denied(path)) {
        return false;
    }
    let now = wf_common::now();
    paths.iter().all(|path| {
        let normalized = normalize_file_path(path);
        grants.iter().any(|grant| {
            !grant.is_expired(now)
                && operation_matches(grant, operation)
                && matches_pattern(&grant.pattern, &normalized)
        })
    })
}

/// Preset grants that may write outside checkpoint tracking. Used by config
/// resolution to admit untracked prefixes into the execution scope.
pub fn untracked_write_prefixes(grants: Option<&[PathGrant]>) -> Vec<String> {
    let now = wf_common::now();
    grants
        .unwrap_or_default()
        .iter()
        .filter(|grant| !grant.is_expired(now))
        .filter(|grant| !grant.checkpointed && grant.operation.allows_write())
        .map(|grant| grant.pattern.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::tool::path_grant::PathGrantOperation;

    fn grant(pattern: &str) -> PathGrant {
        PathGrant {
            pattern: pattern.to_string(),
            operation: PathGrantOperation::ReadWrite,
            preset: None,
            checkpointed: true,
            ttl_secs: None,
            issued_at_ms: None,
            disclosure: wf_types::tool::path_grant::PathDisclosure::Paths,
        }
    }

    #[test]
    fn directory_grant_covers_descendants() {
        let grants = vec![grant("/home/user/.config")];
        assert!(granted(
            Some(&grants),
            &["/home/user/.config/app.toml".to_string()],
            &FileOperationType::Write
        ));
    }

    #[test]
    fn permanent_deny_beats_grant() {
        let grants = vec![grant("/repo")];
        assert!(!granted(
            Some(&grants),
            &["/repo/secrets/key.pem".to_string()],
            &FileOperationType::Read
        ));
    }

    #[test]
    fn expired_grant_is_not_applied() {
        let mut expired = grant("/repo");
        expired.ttl_secs = Some(1);
        expired.issued_at_ms = Some(0);
        assert!(!granted(
            Some(&[expired]),
            &["/repo/a.txt".to_string()],
            &FileOperationType::Write
        ));
    }

    #[test]
    fn write_only_grant_does_not_cover_read() {
        let mut only_write = grant("/repo");
        only_write.operation = PathGrantOperation::Write;
        assert!(!granted(
            Some(&[only_write]),
            &["/repo/a.txt".to_string()],
            &FileOperationType::Read
        ));
    }
}
