use super::*;

/// Deletes every point chunked from one source from both indexes.
/// Returns the vector-side removal count; the keyword side is removed
/// in the same call so the two indexes stay aligned. Both sides are always
/// attempted even when the first one fails, and the first error is returned
/// so no failure is silently masked.
pub async fn delete_source<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    namespace: Option<&str>,
    source: &str,
) -> Result<u64> {
    let vector_outcome = sync_support::remove_source(store, collection, namespace, source).await;
    let keyword_outcome: Result<()> = match keyword {
        Some(keyword) => keyword
            .delete_by_source(collection, namespace, source)
            .await
            .map(|_| ())
            .map_err(Into::into),
        None => Ok(()),
    };
    match (vector_outcome, keyword_outcome) {
        (Ok(removed), Ok(())) => Ok(removed),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

/// Deletes points by id from both indexes. Both sides are always attempted
/// even when the first one fails; the first error is returned.
pub async fn delete_by_ids<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    ids: &[String],
) -> Result<u64> {
    let vector_outcome = sync_support::remove_by_ids(store, collection, ids).await;
    let keyword_outcome: Result<()> = match keyword {
        Some(keyword) => keyword
            .delete_by_id(collection, ids)
            .await
            .map(|_| ())
            .map_err(Into::into),
        None => Ok(()),
    };
    match (vector_outcome, keyword_outcome) {
        (Ok(removed), Ok(())) => Ok(removed),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

/// Per-side point counts for one source, for operations that need to detect
/// a half-synced scope without failing on a missing collection.
pub async fn source_counts<S: VectorStorage + ?Sized, K: KeywordIndex + ?Sized>(
    store: &S,
    keyword: Option<&K>,
    collection: &str,
    namespace: &str,
    source: &str,
) -> Result<(u64, Option<u64>)> {
    let filter = SearchFilter::new()
        .with_namespace(namespace)
        .with_source(source);
    let scope = KeywordScope::new()
        .with_namespace(namespace)
        .with_source(source);
    let vector_count = match store.count(collection, Some(&filter)).await {
        Ok(count) => count,
        Err(vector_contracts::VectorError::CollectionMissing(_)) => 0,
        Err(err) => return Err(err.into()),
    };
    let keyword_count = match keyword {
        Some(keyword) => Some(keyword.count(collection, Some(&scope)).await.unwrap_or(0)),
        None => None,
    };
    Ok((vector_count, keyword_count))
}
