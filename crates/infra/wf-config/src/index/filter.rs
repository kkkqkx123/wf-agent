use super::*;

/// Filter resolved entries by tags. Tags on entries are stored as arrays;
/// all requested tags must be present.
pub fn filter_by_tags(entries: &[ResolvedIndexEntry], tags: &[String]) -> Vec<ResolvedIndexEntry> {
    if tags.is_empty() {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|entry| {
            tags.iter().all(|tag| {
                entry
                    .tags
                    .as_ref()
                    .map(|entry_tags| entry_tags.contains(tag))
                    .unwrap_or(false)
            })
        })
        .cloned()
        .collect()
}

pub fn filter_by_category(
    entries: &[ResolvedIndexEntry],
    category: &str,
) -> Vec<ResolvedIndexEntry> {
    entries
        .iter()
        .filter(|entry| entry.category.as_deref() == Some(category))
        .cloned()
        .collect()
}

pub fn find_entry_by_id<'a>(
    entries: &'a [ResolvedIndexEntry],
    id: &str,
) -> Option<&'a ResolvedIndexEntry> {
    entries.iter().find(|entry| entry.id == id)
}
