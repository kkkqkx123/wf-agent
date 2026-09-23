//! Cursor-style pagination shared by list handlers.
//!
//! Lists return `PageView` (`items` + `limit` + `offset` + `has_more`) instead
//! of bare arrays so the web frontend can page without knowing totals.
//! Totals are intentionally absent: filtered counts are not exposed by the
//! storage adapters, and a fabricated total would be worse than none.
//!
//! Uniform rule: every handler produces a window starting at `offset` with at
//! most `limit + 1` items; the extra item becomes `has_more` and is dropped
//! from `items`. Domain-backed lists request `fetch_size(limit)` from the
//! domain layer; in-memory lists slice the full vector the same way.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::envelope::{ok, ApiEnvelope};
use crate::extract::ListQuery;

/// Default page size when the caller passes no `limit`.
pub(crate) const DEFAULT_PAGE_LIMIT: u64 = 50;
/// Hard cap for a single page; large dumps stay pageable instead of fatal.
pub(crate) const MAX_PAGE_LIMIT: u64 = 500;
/// Hard cap for checkpoint and tool chains; chains are single-parent
/// structures, so they are capped with a truncation flag instead of paged.
pub(crate) const MAX_CHAIN_ENTRIES: usize = 500;
/// Hard cap for per-execution and per-loop timelines and event views.
pub(crate) const MAX_TIMELINE_ENTRIES: usize = 5000;

/// One page of a list response.
#[derive(Serialize, ToSchema)]
pub(crate) struct PageView<T: Serialize> {
    pub(crate) items: Vec<T>,
    pub(crate) limit: u64,
    pub(crate) offset: u64,
    pub(crate) has_more: bool,
}

impl<T: Serialize> PageView<T> {
    /// Build a page from a window starting at `offset` with at most
    /// `limit + 1` items (see module docs).
    pub(crate) fn from_window(mut window: Vec<T>, limit: u64, offset: u64) -> Self {
        let has_more = window.len() as u64 > limit;
        window.truncate(limit as usize);
        Self {
            items: window,
            limit,
            offset,
            has_more,
        }
    }
}

/// Resolve `limit` / `offset` with defaults and the hard cap.
pub(crate) fn resolve_page(query: &ListQuery) -> (u64, u64) {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    (limit, query.offset.unwrap_or(0))
}

/// Fetch size handlers pass to the domain layer: one more than the page.
pub(crate) fn fetch_size(limit: u64) -> u64 {
    limit.saturating_add(1)
}

/// Render a window through the standard envelope.
pub(crate) fn ok_page<T: Serialize>(
    window: Vec<T>,
    limit: u64,
    offset: u64,
) -> Json<ApiEnvelope<PageView<T>>> {
    ok(PageView::from_window(window, limit, offset))
}

/// Capped list view for chains and timelines: full structure up to a hard
/// cap with an explicit truncation flag and pre-truncation total.
#[derive(Serialize, ToSchema)]
pub(crate) struct CappedView<T: Serialize> {
    pub(crate) items: Vec<T>,
    pub(crate) truncated: bool,
    pub(crate) total: usize,
}

impl<T: Serialize> CappedView<T> {
    pub(crate) fn from_all(mut all: Vec<T>, cap: usize) -> Self {
        let total = all.len();
        let truncated = total > cap;
        all.truncate(cap);
        Self {
            items: all,
            truncated,
            total,
        }
    }
}

/// Render a full list through the capped view.
pub(crate) fn ok_capped<T: Serialize>(all: Vec<T>, cap: usize) -> Json<ApiEnvelope<CappedView<T>>> {
    ok(CappedView::from_all(all, cap))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_splits_has_more() {
        let page = PageView::from_window(vec![1, 2, 3], 2, 0);
        assert_eq!(page.items, vec![1, 2]);
        assert!(page.has_more);
        assert_eq!((page.limit, page.offset), (2, 0));
    }

    #[test]
    fn exact_page_has_no_more() {
        let page = PageView::<i32>::from_window(vec![1, 2], 2, 4);
        assert!(!page.has_more);
        assert_eq!(page.offset, 4);
    }

    #[test]
    fn resolve_applies_defaults_and_cap() {
        let query = ListQuery {
            limit: None,
            offset: None,
        };
        assert_eq!(resolve_page(&query), (DEFAULT_PAGE_LIMIT, 0));
        let query = ListQuery {
            limit: Some(u64::MAX),
            offset: Some(7),
        };
        assert_eq!(resolve_page(&query), (MAX_PAGE_LIMIT, 7));
        let query = ListQuery {
            limit: Some(0),
            offset: None,
        };
        assert_eq!(resolve_page(&query).0, 1);
    }
}
