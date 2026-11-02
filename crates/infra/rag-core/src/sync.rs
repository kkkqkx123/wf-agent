//! Source-level sync: content hashes, update plans and removals.
//!
//! The ingest pipeline applies the write policy per source; this module
//! adds the caller-side sync semantics: decide whether a source needs
//! re-indexing by comparing content hashes, and remove sources or id
//! lists through the storage contract.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Result;
use vector_contracts::{SearchFilter, VectorStorage};

/// Sync decision for one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncAction {
    /// Source is unknown: index it.
    Insert,
    /// Content hash changed: delete the source scope, then index.
    Update,
    /// Content hash matches the indexed state: skip.
    Skip,
}

/// One source sync decision with its hashes for logging.
#[derive(Debug, Clone)]
pub struct SyncDecision {
    /// Source label the decision applies to.
    pub source: String,
    /// Action to take.
    pub action: SyncAction,
    /// Hash of the incoming content.
    pub incoming_hash: String,
    /// Hash of the indexed content, when known.
    pub indexed_hash: Option<String>,
}

/// Stable hex hash of document content for change detection.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Plans sync actions by comparing incoming hashes against indexed hashes.
/// Sources absent from `indexed` are inserts; equal hashes are skips.
pub fn plan_sync(
    incoming: &HashMap<String, String>,
    indexed: &HashMap<String, String>,
) -> Vec<SyncDecision> {
    let mut decisions: Vec<SyncDecision> = incoming
        .iter()
        .map(|(source, hash)| {
            let action = match indexed.get(source) {
                None => SyncAction::Insert,
                Some(known) if known == hash => SyncAction::Skip,
                Some(_) => SyncAction::Update,
            };
            SyncDecision {
                source: source.clone(),
                action,
                incoming_hash: hash.clone(),
                indexed_hash: indexed.get(source).cloned(),
            }
        })
        .collect();
    decisions.sort_by(|a, b| a.source.cmp(&b.source));
    decisions
}

/// Deletes every point chunked from one source, optionally scoped to a
/// namespace. Returns the number of removed points.
pub async fn remove_source<S: VectorStorage + ?Sized>(
    store: &S,
    collection: &str,
    namespace: Option<&str>,
    source: &str,
) -> Result<u64> {
    store
        .delete_by_source(collection, namespace, source)
        .await
        .map_err(Into::into)
}

/// Deletes points by id. Returns the number of removed points.
pub async fn remove_by_ids<S: VectorStorage + ?Sized>(
    store: &S,
    collection: &str,
    ids: &[String],
) -> Result<u64> {
    store
        .delete_by_id(collection, ids)
        .await
        .map_err(Into::into)
}

/// Hashes the incoming sources that are missing from `known` so callers
/// can persist the hash alongside the index state after a successful
/// ingest. Returns the hashes for the given sources in source order.
pub fn hash_sources(sources: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = sources
        .iter()
        .map(|(source, content)| (source.to_string(), content_hash(content)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Filters decisions down to the sources needing (re-)indexing.
pub fn pending_sources(decisions: &[SyncDecision]) -> HashSet<String> {
    decisions
        .iter()
        .filter(|decision| !matches!(decision.action, SyncAction::Skip))
        .map(|decision| decision.source.clone())
        .collect()
}

/// Counts indexed points for one source, used to check index presence
/// before planning an update.
pub async fn source_point_count<S: VectorStorage + ?Sized>(
    store: &S,
    collection: &str,
    namespace: &str,
    source: &str,
) -> Result<u64> {
    let filter = SearchFilter::new()
        .with_namespace(namespace)
        .with_source(source);
    store
        .count(collection, Some(&filter))
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_stable_and_content_sensitive() {
        assert_eq!(content_hash("hello"), content_hash("hello"));
        assert_ne!(content_hash("hello"), content_hash("world"));
    }

    #[test]
    fn plan_distinguishes_insert_update_skip() {
        let incoming = HashMap::from([
            ("new.md".to_string(), "h1".to_string()),
            ("changed.md".to_string(), "h2".to_string()),
            ("same.md".to_string(), "h3".to_string()),
        ]);
        let indexed = HashMap::from([
            ("changed.md".to_string(), "old".to_string()),
            ("same.md".to_string(), "h3".to_string()),
        ]);
        let decisions = plan_sync(&incoming, &indexed);
        let by_source: HashMap<&str, &SyncAction> = decisions
            .iter()
            .map(|decision| (decision.source.as_str(), &decision.action))
            .collect();
        assert_eq!(by_source["new.md"], &SyncAction::Insert);
        assert_eq!(by_source["changed.md"], &SyncAction::Update);
        assert_eq!(by_source["same.md"], &SyncAction::Skip);
        let pending = pending_sources(&decisions);
        assert!(pending.contains("new.md"));
        assert!(pending.contains("changed.md"));
        assert!(!pending.contains("same.md"));
    }
}
