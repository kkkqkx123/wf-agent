//! Incremental preparation cache for the scrollback area.
//!
//! The interactive draw path used to rewrap every history line on each frame.
//! This cache keeps the wrapped display rows plus a per-source index so steady
//! streaming frames only lay out the newly added source lines. Width changes
//! trigger a full relayout; theme and animation changes never invalidate the
//! layout. The in-flight streaming line is intentionally excluded and rendered
//! separately each frame.
//!
//! This module re-exports from focused sub-modules:
//! - Display-row intervals (`RowRange`)
//! - Oversize frame retention (`OversizeCache`)
//! - Row-level message cache (`RowCache`)
//! - Auxiliary rendered-to-logical map (`AuxLineMap`, `AuxEntry`)
//! - Committed scrollback preparation (`PreparedScrollback`)

pub mod aux_map;
pub mod oversize;
pub mod row_cache;
pub mod row_range;
pub mod scrollback;

pub use aux_map::{AuxEntry, AuxLineMap, AUX_MAP_MAX_ENTRIES};
pub use oversize::{OversizeCache, OVERSIZE_CACHE_MAX_ENTRIES};
pub use row_cache::{RowCache, ROW_CACHE_MAX_ENTRIES};
pub use row_range::RowRange;
pub use scrollback::PreparedScrollback;

#[cfg(test)]
mod tests {
    use super::*;

    use ratatui::text::Line;
    use tui_components::transcript::{HistoryLine, LineState, Role};
    use tui_core::prep_keys::AuxMapKey;

    fn line(text: &str) -> HistoryLine {
        HistoryLine::new_with_role(text, LineState::Committed, Role::Default)
    }

    fn full_layout(full: &[HistoryLine], width: u16) -> Vec<String> {
        full.iter()
            .flat_map(|line| line.display_lines(width))
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn cached_text(cache: &PreparedScrollback) -> Vec<String> {
        cache
            .rows()
            .iter()
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn assert_index_consistent(cache: &PreparedScrollback) {
        let mut expected = 0usize;
        for range in cache.index() {
            assert_eq!(range.start, expected);
            expected += range.len;
        }
        assert_eq!(expected, cache.total_rows());
        assert_eq!(cache.index().len(), cache.source_len());
    }

    #[test]
    fn append_lays_out_only_new_lines() {
        let mut cache = PreparedScrollback::new();
        let first = vec![line("alpha beta gamma delta"), line("second line here")];
        cache.sync_replace(&first, 10, 1);
        assert_eq!(cache.last_layout_count(), 2);
        let mut full = first.clone();
        full.push(line("brand new tail line"));
        cache.sync_append(&full, 10, 2);
        assert_eq!(cache.last_layout_count(), 1);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_index_consistent(&cache);
    }

    #[test]
    fn prepend_keeps_anchor_and_index() {
        let mut cache = PreparedScrollback::new();
        let tail = vec![line("tail one two three"), line("tail two")];
        cache.sync_replace(&tail, 10, 1);
        let old_start = cache.source_row_start(0).unwrap_or(0);
        let mut full = vec![line("head line newcomer")];
        full.extend(tail.clone());
        let shift = cache.sync_prepend(&full, 1, 10, 2);
        assert_eq!(cache.last_layout_count(), 1);
        assert_eq!(shift, cache.source_row_start(1).unwrap_or(0));
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_index_consistent(&cache);
        let shifted = cache.source_row_start(1).unwrap_or(0);
        assert!(shifted > old_start || old_start == 0);
    }

    #[test]
    fn trim_drops_head_without_relayout() {
        let mut cache = PreparedScrollback::new();
        let full: Vec<HistoryLine> = (0..5)
            .map(|i| line(&format!("line number {i} content")))
            .collect();
        cache.sync_replace(&full, 12, 1);
        cache.sync_trim(2, 2);
        assert_eq!(cache.last_layout_count(), 0);
        assert_eq!(cached_text(&cache), full_layout(&full[2..], 12));
        assert_index_consistent(&cache);
    }

    #[test]
    fn width_change_falls_back_to_full() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("some longer content for wrapping")];
        cache.sync_replace(&full, 10, 1);
        let narrow_rows = cache.total_rows();
        cache.sync_append(&full, 30, 2);
        assert_eq!(cached_text(&cache), full_layout(&full, 30));
        assert!(cache.total_rows() <= narrow_rows);
    }

    #[test]
    fn random_sequence_matches_full_relayout() {
        let mut cache = PreparedScrollback::new();
        let mut full: Vec<HistoryLine> = Vec::new();
        let mut version = 0u64;
        let width = 14u16;
        // Deterministic operation mix: append, prepend, trim, replace.
        for step in 0..40usize {
            version += 1;
            match step % 5 {
                0 | 1 => {
                    full.push(line(&format!("append step {step} with words")));
                    cache.sync_append(&full, width, version);
                }
                2 => {
                    let head = vec![
                        line(&format!("head {step}")),
                        line(&format!("head2 {step}")),
                    ];
                    let mut next = head.clone();
                    next.extend(full.clone());
                    full = next;
                    cache.sync_prepend(&full, head.len(), width, version);
                }
                3 if full.len() > 3 => {
                    let drop = 2usize;
                    full.drain(0..drop);
                    cache.sync_trim(drop, version);
                }
                _ => {
                    full = vec![line(&format!("replaced at {step}")), line("fresh tail")];
                    cache.sync_replace(&full, width, version);
                }
            }
            assert_eq!(
                cached_text(&cache),
                full_layout(&full, width),
                "step {step}"
            );
            assert_index_consistent(&cache);
        }
    }

    #[test]
    fn replace_identical_content_reuses_row_cache() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("alpha beta gamma delta"), line("second line here")];
        cache.sync_replace(&full, 10, 1);
        assert_eq!(cache.last_layout_count(), 2);
        assert!(!cache.row_cache().is_empty());
        cache.sync_replace(&full, 10, 2);
        assert_eq!(cache.last_layout_count(), 0);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_index_consistent(&cache);
    }

    #[test]
    fn aux_map_survives_replace_without_reparsing() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("assistant answer one two three")];
        cache.sync_replace(&full, 10, 1);
        let identity = full[0].identity();
        let recorded = cache.aux_map().display_len(identity, 10);
        assert_eq!(recorded, Some(cache.total_rows()));
        cache.sync_replace(&full, 10, 2);
        assert_eq!(cache.aux_map().display_len(identity, 10), recorded);
        assert_eq!(cache.aux_map().display_len(identity, 30), None);
    }

    #[test]
    fn width_change_clears_row_cache_but_text_matches() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("some longer content for wrapping")];
        cache.sync_replace(&full, 10, 1);
        assert!(!cache.row_cache().is_empty());
        cache.sync_replace(&full, 30, 2);
        assert_eq!(cached_text(&cache), full_layout(&full, 30));
        assert_eq!(cache.last_layout_count(), 1);
    }

    #[test]
    fn row_cache_evicts_oldest_generation_when_full() {
        let mut row_cache = RowCache::new();
        for i in 0..=ROW_CACHE_MAX_ENTRIES {
            row_cache.insert(
                AuxMapKey::new(i as u64, 80),
                vec![Line::from(format!("row {i}"))],
            );
        }
        assert!(row_cache.len() <= ROW_CACHE_MAX_ENTRIES);
        assert!(row_cache
            .get(AuxMapKey::new(ROW_CACHE_MAX_ENTRIES as u64, 80))
            .is_some());
    }

    #[test]
    fn hidden_mismatch_forces_full_relayout() {
        let mut cache = PreparedScrollback::new();
        let tail = vec![line("tail one two three"), line("tail two")];
        cache.sync_replace_with_hidden(&tail, 10, 1, 0);
        let mut full = vec![line("head line newcomer")];
        full.extend(tail.clone());
        let shift = cache.sync_prepend_with_hidden(&full, 1, 10, 2, 3);
        assert_eq!(cache.hidden(), 3);
        assert_eq!(cached_text(&cache), full_layout(&full, 10));
        assert_eq!(shift, cache.source_row_start(1).unwrap_or(0));
    }

    #[test]
    fn generational_eviction_preserves_recent_hits() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("alpha beta gamma delta"), line("second line here")];
        cache.sync_replace(&full, 10, 1);
        cache.sync_replace(&full, 10, 2);
        assert_eq!(cache.last_layout_count(), 0);
        assert!(!cache.row_cache().is_empty());
    }

    #[test]
    fn tail_edit_rebuilds_only_body() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("head pinned"), line("body one"), line("tail old")];
        cache.sync_replace(&full, 12, 1);
        cache.set_header_len(1);
        let mut edited = full.clone();
        edited[2] = line("tail new content here");
        cache.sync_append_with_hidden(&edited, 12, 2, 0);
        assert_eq!(cached_text(&cache), full_layout(&edited, 12));
        assert_eq!(cache.last_layout_count(), 1);
        assert_index_consistent(&cache);
    }

    #[test]
    fn header_body_keys_split_hidden() {
        let mut cache = PreparedScrollback::new();
        let full = vec![line("head"), line("body")];
        cache.sync_replace_with_hidden(&full, 10, 7, 3);
        assert_eq!(cache.header_key().width, 10);
        assert_eq!(cache.header_key().version, 7);
        assert_eq!(cache.body_key().hidden, 3);
        assert_eq!(cache.key().body_key(), cache.body_key());
        assert_eq!(cache.key().header_key(), cache.header_key());
    }

    #[test]
    fn discontinuous_seq_forces_full_relayout() {
        let mut cache = PreparedScrollback::new();
        let mut tail = vec![line("old tail one"), line("old tail two")];
        tail[0].set_seq_no(10);
        tail[1].set_seq_no(11);
        cache.sync_replace(&tail, 12, 1);
        let mut head = vec![line("new head")];
        head[0].set_seq_no(1);
        let mut full = head;
        full.extend(tail.clone());
        let shift = cache.sync_prepend_with_hidden(&full, 1, 12, 2, 0);
        assert_eq!(cached_text(&cache), full_layout(&full, 12));
        assert_eq!(shift, cache.source_row_start(1).unwrap_or(0));
        assert_index_consistent(&cache);
    }

    #[test]
    fn streaming_settle_hits_row_cache() {
        let mut cache = PreparedScrollback::new();
        let streaming =
            HistoryLine::new_with_role("same text", LineState::Streaming, Role::Default);
        let committed =
            HistoryLine::new_with_role("same text", LineState::Committed, Role::Default);
        assert_eq!(streaming.identity(), committed.identity());
        let full = vec![committed];
        cache.sync_replace(&full, 12, 1);
        assert_eq!(cache.last_layout_count(), 1);
        cache.sync_replace(&full, 12, 2);
        assert_eq!(cache.last_layout_count(), 0);
    }
}
