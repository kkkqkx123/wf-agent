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
    resolve_page_fields(query.limit, query.offset)
}

/// Resolve raw `limit` / `offset` fields with defaults and the hard cap.
pub(crate) fn resolve_page_fields(limit: Option<u64>, offset: Option<u64>) -> (u64, u64) {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
    (limit, offset.unwrap_or(0))
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

/// One cursor page of a list response.
///
/// `next_cursor` is opaque: callers pass it back verbatim and never parse
/// it. It currently encodes the next numeric offset, but that shape is an
/// internal detail and may change without notice.
#[derive(Serialize, ToSchema)]
pub(crate) struct CursorPageView<T: Serialize> {
    pub(crate) items: Vec<T>,
    pub(crate) limit: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) next_cursor: Option<String>,
    pub(crate) has_more: bool,
}

impl<T: Serialize> CursorPageView<T> {
    /// Build a cursor page from a window starting at `offset` with at most
    /// `limit + 1` items; the extra item becomes `has_more` and the cursor
    /// for the following page.
    pub(crate) fn from_window(mut window: Vec<T>, limit: u64, offset: u64) -> Self {
        let has_more = window.len() as u64 > limit;
        window.truncate(limit as usize);
        let next_cursor = has_more.then(|| encode_cursor(offset.saturating_add(limit)));
        Self {
            items: window,
            limit,
            next_cursor,
            has_more,
        }
    }
}

/// Encode a numeric offset as an opaque cursor.
pub(crate) fn encode_cursor(offset: u64) -> String {
    offset.to_string()
}

/// Decode an opaque cursor back to its numeric offset.
pub(crate) fn decode_cursor(cursor: &str) -> Result<u64, String> {
    cursor
        .parse::<u64>()
        .map_err(|_| format!("unknown cursor: {cursor}"))
}

/// Resolve `limit` with defaults and the hard cap plus an opaque `cursor`.
pub(crate) fn resolve_cursor_page(
    limit: Option<u64>,
    cursor: Option<&str>,
) -> Result<(u64, u64), String> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
    let offset = match cursor {
        None | Some("") => 0,
        Some(raw) => decode_cursor(raw)?,
    };
    Ok((limit, offset))
}

/// Render a window through the cursor-page envelope.
pub(crate) fn ok_cursor_page<T: Serialize>(
    window: Vec<T>,
    limit: u64,
    offset: u64,
) -> Json<ApiEnvelope<CursorPageView<T>>> {
    ok(CursorPageView::from_window(window, limit, offset))
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
    fn cursor_round_trips_offset() {
        assert_eq!(decode_cursor(&encode_cursor(0)), Ok(0));
        assert_eq!(decode_cursor(&encode_cursor(42)), Ok(42));
        assert!(decode_cursor("not-a-cursor").is_err());
        assert_eq!(resolve_cursor_page(None, None), Ok((DEFAULT_PAGE_LIMIT, 0)));
        assert_eq!(
            resolve_cursor_page(Some(10), Some(&encode_cursor(20))),
            Ok((10, 20))
        );
        assert!(resolve_cursor_page(None, Some("bogus")).is_err());
    }

    #[test]
    fn cursor_window_splits_has_more() {
        let page = CursorPageView::from_window(vec![1, 2, 3], 2, 0);
        assert_eq!(page.items, vec![1, 2]);
        assert!(page.has_more);
        assert_eq!(page.next_cursor, Some("2".to_string()));
        let last = CursorPageView::from_window(vec![3], 2, 2);
        assert!(!last.has_more);
        assert_eq!(last.next_cursor, None);
    }

    #[test]
    fn resolve_applies_defaults_and_cap() {
        assert_eq!(resolve_page_fields(None, None), (DEFAULT_PAGE_LIMIT, 0));
        assert_eq!(
            resolve_page_fields(Some(u64::MAX), Some(7)),
            (MAX_PAGE_LIMIT, 7)
        );
        assert_eq!(resolve_page_fields(Some(0), None).0, 1);
        let query = ListQuery {
            limit: Some(3),
            offset: Some(1),
        };
        assert_eq!(resolve_page(&query), (3, 1));
    }
}
