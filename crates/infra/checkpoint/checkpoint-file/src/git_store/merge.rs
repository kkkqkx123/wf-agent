use crate::git_store::{
    GitStore, GitStoreError, REF_FEAT_PREFIX, REF_MAIN, REF_REVIEW_PREFIX,
    STATE_CONFLICT_UNRESOLVED, TRAILER_CONFLICT_FILE, TRAILER_STATE,
};
use std::collections::{HashMap, HashSet};

/// One merged file: bytes (`None` = deleted) plus conflict flag.
pub struct MergeFileOutcome {
    pub bytes: Option<Vec<u8>>,
    pub conflicted: bool,
}

/// Whether bytes look binary (NUL in the probed prefix).
pub(crate) fn is_binary_bytes(bytes: &[u8]) -> bool {
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
    // Overlapping changes: line-based text merge with diff3-style markers
    // (absent sides read as empty). The ancestor label is set so conflict
    // regions embed their base section and read views can recover the true
    // base, ours and theirs intervals instead of whole-file sides.
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
        ancestor: Some(gix_object::bstr::BStr::new("base")),
        current: Some(gix_object::bstr::BStr::new("ours")),
        other: Some(gix_object::bstr::BStr::new("theirs")),
    };
    let options = gix_merge::blob::builtin_driver::text::Options {
        conflict:
            gix_merge::blob::builtin_driver::text::Conflict::Keep {
                style: gix_merge::blob::builtin_driver::text::ConflictStyle::Diff3,
                marker_size: std::num::NonZeroU8::new(
                    gix_merge::blob::builtin_driver::text::Conflict::DEFAULT_MARKER_SIZE,
                )
                .expect("default marker size is non-zero"),
            },
        ..Default::default()
    };
    let resolution = gix_merge::blob::builtin_driver::text(
        &mut merged,
        &mut input,
        labels,
        ours_bytes,
        base_bytes,
        theirs_bytes,
        options,
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

impl GitStore {
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
