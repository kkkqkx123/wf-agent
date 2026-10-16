//! On-demand diagram raster metadata: viewport cache, width buckets,
//! font metrics and scaling.
//!
//! No raster backend is bundled: actual pixels come from an external vector
//! toolchain only when the product enables [`RasterConfig`]. Every entry
//! here is metadata (hashes plus cell geometry), so the cache stays small
//! and observable through hit/miss counters. Placeholder geometry never
//! changes with or without a backend: [`placeholder_size`] applies the same
//! clamp as [`crate::deferred::placeholder_lines`]. Backend completions only
//! signal through [`mix_signature`]: the shell folds the returned signature
//! into its redraw snapshot, where the existing image-signature path forces
//! a full frame.

use std::collections::{HashMap, VecDeque};

/// Width bucket in cells quantizing raster requests. Tiny pane changes such
/// as a one-cell scrollbar reservation reuse the same cached rendition.
pub const RASTER_WIDTH_BUCKET_CELLS: u32 = 4;

/// Default bound for resident raster metadata entries.
pub const RASTER_CACHE_MAX_ENTRIES: usize = 64;

/// Assumed terminal cell size in pixels when no measurement is available.
pub const DEFAULT_CELL_PX_W: u16 = 8;
/// Assumed terminal cell height in pixels.
pub const DEFAULT_CELL_PX_H: u16 = 16;

/// Independent raster switch plus cache shaping. Disabled by default so
/// output matches the placeholder path until a product opts in; any failure
/// below falls back to the same placeholder geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RasterConfig {
    /// Run the external vector rasterizer and serve cached renditions.
    pub enabled: bool,
    /// Width bucket in cells (0 disables bucketing).
    pub width_bucket: u32,
    /// Maximum resident metadata entries.
    pub max_entries: usize,
}

impl Default for RasterConfig {
    /// Default off with reference bucketing and bounding.
    fn default() -> Self {
        Self {
            enabled: false,
            width_bucket: RASTER_WIDTH_BUCKET_CELLS,
            max_entries: RASTER_CACHE_MAX_ENTRIES,
        }
    }
}

impl RasterConfig {
    /// Explicit opt-in with default shaping.
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }
}

/// Terminal cell size in pixels driving cell-to-pixel scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontMetrics {
    /// Cell width in pixels.
    pub cell_px_w: u16,
    /// Cell height in pixels.
    pub cell_px_h: u16,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            cell_px_w: DEFAULT_CELL_PX_W,
            cell_px_h: DEFAULT_CELL_PX_H,
        }
    }
}

/// Minimal font registry: named cell metrics plus a prewarm marker. The
/// marker records that a font scan was requested without paying for one at
/// startup; unknown families resolve to none so callers keep placeholders.
#[derive(Debug, Clone, Default)]
pub struct FontLibrary {
    warmed: bool,
}

impl FontLibrary {
    /// Empty registry (cold font database).
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a background font-database load request.
    pub fn prewarm(&mut self) {
        self.warmed = true;
    }

    /// True after [`Self::prewarm`].
    pub fn is_warmed(self) -> bool {
        self.warmed
    }

    /// Cell metrics for a known family, if registered.
    pub fn get(self, name: &str) -> Option<FontMetrics> {
        match name.trim().to_ascii_lowercase().as_str() {
            "default" | "mono" | "monospace" => Some(FontMetrics::default()),
            _ => None,
        }
    }
}

/// Quantize a cell width to `bucket` multiples (ceiling), so a one-cell
/// reservation change reuses the same rendition. Zero width stays zero;
/// zero bucket disables bucketing.
pub fn bucket_width(cells: u32, bucket: u32) -> u32 {
    if cells == 0 || bucket == 0 {
        return cells;
    }
    cells
        .saturating_add(bucket.saturating_sub(1))
        .checked_div(bucket)
        .and_then(|quot| quot.checked_mul(bucket))
        .unwrap_or(cells)
}

/// Display size in cells preserving aspect ratio inside the cell box.
/// Terminal cells are roughly twice as tall as wide, hence the aspect
/// compensation. Unknown sources fall back to a bounded default.
pub fn calculate_display_size(img_w: u32, img_h: u32, max_cols: u16, max_rows: u16) -> (u16, u16) {
    if img_w == 0 || img_h == 0 {
        return (max_cols.clamp(1, 40), max_rows.clamp(1, 20));
    }
    let cell_aspect = 2.0_f64;
    let img_aspect = f64::from(img_w) / f64::from(img_h.max(1));
    let max_w = f64::from(max_cols.max(1));
    let max_h = f64::from(max_rows.max(1)) * cell_aspect;
    let (mut display_w, mut display_h) = if img_aspect > max_w / max_h.max(0.001) {
        (max_w, max_w / img_aspect.max(0.001))
    } else {
        (max_h * img_aspect, max_h)
    };
    display_w = display_w.clamp(10.0, max_w);
    display_h = (display_h / cell_aspect).clamp(1.0, f64::from(max_rows.max(1)));
    (display_w as u16, display_h as u16)
}

/// Placeholder box size under the same clamp as the text placeholder path,
/// so geometry is identical with or without a backend.
pub fn placeholder_size(cols: usize, rows: usize) -> (usize, usize) {
    (cols.max(4), rows.max(3))
}

/// Viewport cache key: content plus everything affecting geometry. The
/// bucketed width absorbs tiny resizes; aspect and font complete the
/// identity so a font or pane-shape change never reuses stale pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RasterKey {
    /// Content identity (diagram source hash).
    pub content_hash: u64,
    /// Bucketed target width in cells.
    pub bucketed_width: u32,
    /// Diagram aspect bucket from the unified layout entry point.
    pub aspect_bucket: u8,
    /// Cell width in pixels.
    pub font_w: u16,
    /// Cell height in pixels.
    pub font_h: u16,
}

impl RasterKey {
    /// Key for `content_hash` at `target_cols` under `font`.
    pub fn new(content_hash: u64, target_cols: u32, aspect_bucket: u8, font: FontMetrics) -> Self {
        Self {
            content_hash,
            bucketed_width: bucket_width(target_cols, RASTER_WIDTH_BUCKET_CELLS),
            aspect_bucket,
            font_w: font.cell_px_w.max(1),
            font_h: font.cell_px_h.max(1),
        }
    }
}

/// Cached rendition metadata: cell geometry plus backing pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachedRaster {
    /// Display width in cells.
    pub cols: u16,
    /// Display height in cells.
    pub rows: u16,
    /// Backing width in pixels.
    pub px_w: u32,
    /// Backing height in pixels.
    pub px_h: u32,
}

/// Bounded viewport rendition cache with observable hit accounting.
#[derive(Debug, Clone, Default)]
pub struct RasterCache {
    entries: HashMap<RasterKey, CachedRaster>,
    order: VecDeque<RasterKey>,
    hits: u64,
    misses: u64,
    max_entries: usize,
}

impl RasterCache {
    /// Empty cache bounded by `max_entries` (0 means the default bound).
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            hits: 0,
            misses: 0,
            max_entries: if max_entries == 0 {
                RASTER_CACHE_MAX_ENTRIES
            } else {
                max_entries
            },
        }
    }

    /// Lookup promoting hits to most-recent. Misses only count; the caller
    /// rasterizes externally and inserts on success.
    pub fn lookup(&mut self, key: &RasterKey) -> Option<CachedRaster> {
        if let Some(raster) = self.entries.get(key).copied() {
            if let Some(pos) = self.order.iter().position(|entry| entry == key) {
                self.order.remove(pos);
            }
            self.order.push_back(*key);
            self.hits = self.hits.saturating_add(1);
            Some(raster)
        } else {
            self.misses = self.misses.saturating_add(1);
            None
        }
    }

    /// Insert evicting least-recent entries beyond the bound.
    pub fn insert(&mut self, key: RasterKey, raster: CachedRaster) {
        match self.entries.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                occupied.insert(raster);
                if let Some(pos) = self.order.iter().position(|entry| *entry == key) {
                    self.order.remove(pos);
                }
                self.order.push_back(key);
                return;
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(raster);
            }
        }
        self.order.push_back(key);
        while self.order.len() > self.max_entries.max(1) {
            if let Some(old) = self.order.pop_front() {
                self.entries.remove(&old);
            } else {
                break;
            }
        }
    }

    /// Resident entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no rendition is retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `(hits, misses, resident entries)` for regression reports.
    pub fn counts(&self) -> (u64, u64, usize) {
        (self.hits, self.misses, self.entries.len())
    }

    /// Hit fraction over all lookups (0 with no lookups).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits.saturating_add(self.misses);
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Pixel scaling for a source fitting a cell box at `font`, preserving
/// aspect ratio. Pure metadata: no decoding happens here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalingMetadata {
    /// Scaled width in pixels.
    pub px_w: u32,
    /// Scaled height in pixels.
    pub px_h: u32,
    /// Display width in cells.
    pub cols: u16,
    /// Display height in cells.
    pub rows: u16,
}

/// Scale `img_w` by `img_h` into the `target_cols` by `target_rows` box.
pub fn scale_to_fit(
    img_w: u32,
    img_h: u32,
    target_cols: u16,
    target_rows: u16,
    font: FontMetrics,
) -> Option<ScalingMetadata> {
    if img_w == 0 || img_h == 0 || target_cols == 0 || target_rows == 0 {
        return None;
    }
    let font_w = u32::from(font.cell_px_w.max(1));
    let font_h = u32::from(font.cell_px_h.max(1));
    let max_w = u32::from(target_cols).saturating_mul(font_w);
    let max_h = u32::from(target_rows).saturating_mul(font_h);
    if max_w == 0 || max_h == 0 {
        return None;
    }
    let scale = (f64::from(max_w) / f64::from(img_w)).min(f64::from(max_h) / f64::from(img_h));
    let px_w = ((f64::from(img_w) * scale).round() as u32).max(1);
    let px_h = ((f64::from(img_h) * scale).round() as u32).max(1);
    let cols = px_w
        .saturating_add(font_w.saturating_sub(1))
        .checked_div(font_w)
        .unwrap_or(1)
        .min(u32::from(u16::MAX)) as u16;
    let rows = px_h
        .saturating_add(font_h.saturating_sub(1))
        .checked_div(font_h)
        .unwrap_or(1)
        .min(u32::from(u16::MAX)) as u16;
    Some(ScalingMetadata {
        px_w,
        px_h,
        cols: cols.max(1),
        rows: rows.max(1),
    })
}

/// What feeds the external vector rasterizer (SVG text in, PNG bytes out).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorSource {
    /// Mermaid diagram source.
    Mermaid,
    /// TeX display-math source.
    Tex,
}

/// External vector-rasterizer argv for `svg_path` at `width_px`. Pure
/// construction: executed only when enabled, otherwise placeholders stay.
pub fn vector_raster_argv(svg_path: &str, width_px: u32, source: VectorSource) -> Vec<String> {
    let _ = source;
    vec![
        "convert".to_string(),
        svg_path.to_string(),
        "-resize".to_string(),
        format!("{}x", width_px.max(1)),
        "-colors".to_string(),
        "256".to_string(),
        "png:-".to_string(),
    ]
}

/// True only when the product enabled rasterization and the terminal offers
/// a protocol. Anything else keeps the text placeholder.
pub fn should_use_raster(config: &RasterConfig, protocol_available: bool) -> bool {
    config.enabled && protocol_available
}

/// Advance a backend epoch counter (0 becomes 1, then saturates upward).
pub fn next_epoch(epoch: u64) -> u64 {
    epoch.saturating_add(1).max(1)
}

/// Mix a backend epoch into a content hash for the redraw image signature.
/// Deterministic FNV-1a over both inputs; the shell stores the result where
/// the existing signature comparison already forces a full frame.
pub fn mix_signature(base: u64, epoch: u64) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in base.to_le_bytes().iter().chain(epoch.to_le_bytes().iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deferred::placeholder_lines;

    #[test]
    fn default_config_stays_on_placeholders() {
        let config = RasterConfig::default();
        assert!(!config.enabled);
        assert!(!should_use_raster(&config, true));
        assert!(!should_use_raster(&RasterConfig::enabled(), false));
        assert!(should_use_raster(&RasterConfig::enabled(), true));
    }

    #[test]
    fn width_bucket_absorbs_one_cell_reservation_changes() {
        assert_eq!(bucket_width(80, 4), 80);
        assert_eq!(bucket_width(79, 4), 80);
        assert_eq!(bucket_width(0, 4), 0);
        assert_eq!(bucket_width(79, 0), 79);
        let font = FontMetrics::default();
        assert_eq!(
            RasterKey::new(7, 80, 3, font),
            RasterKey::new(7, 79, 3, font)
        );
        assert_ne!(
            RasterKey::new(7, 80, 3, font),
            RasterKey::new(7, 80, 4, font)
        );
    }

    #[test]
    fn display_size_stays_inside_the_cell_box() {
        let (cols, rows) = calculate_display_size(1920, 1080, 80, 24);
        assert!(cols <= 80 && rows <= 24, "got {cols}x{rows}");
        let (cols, rows) = calculate_display_size(1080, 1920, 80, 24);
        assert!(cols <= 80 && rows <= 24, "got {cols}x{rows}");
        let (cols, rows) = calculate_display_size(0, 0, 80, 24);
        assert!(cols <= 80 && rows <= 24);
    }

    #[test]
    fn placeholder_geometry_matches_without_backend() {
        for (cols, rows) in [(20usize, 5usize), (4, 3), (2, 2), (80, 24)] {
            let (want_w, want_h) = placeholder_size(cols, rows);
            let lines = placeholder_lines("flow", "hint", cols, rows);
            assert_eq!(lines.len(), want_h, "rows for {cols}x{rows}");
            for line in &lines {
                assert_eq!(line.len(), want_w, "line was {line:?}");
            }
        }
    }

    #[test]
    fn cache_reports_hits_and_evicts_lru() {
        let mut cache = RasterCache::new(2);
        let font = FontMetrics::default();
        let first = RasterKey::new(1, 80, 0, font);
        let second = RasterKey::new(2, 80, 0, font);
        let third = RasterKey::new(3, 80, 0, font);
        let raster = CachedRaster {
            cols: 40,
            rows: 12,
            px_w: 320,
            px_h: 192,
        };
        assert!(cache.lookup(&first).is_none());
        cache.insert(first, raster);
        cache.insert(second, raster);
        assert!(cache.lookup(&first).is_some());
        cache.insert(third, raster);
        assert_eq!(cache.len(), 2);
        assert!(cache.lookup(&second).is_none());
        let (hits, misses, _) = cache.counts();
        assert!(hits >= 1 && misses >= 2, "hits={hits} misses={misses}");
        assert!((0.0..=1.0).contains(&cache.hit_rate()));
        assert!(cache.hit_rate() > 0.0);
    }

    #[test]
    fn scaling_preserves_aspect_inside_the_box() {
        let font = FontMetrics::default();
        let scaled = scale_to_fit(400, 200, 40, 24, font).expect("fits");
        assert!(u32::from(scaled.cols) <= 40);
        assert!(u32::from(scaled.rows) <= 24);
        let ratio = f64::from(scaled.px_w) / f64::from(scaled.px_h.max(1));
        assert!((ratio - 2.0).abs() < 0.15, "ratio was {ratio}");
        assert!(scale_to_fit(0, 200, 40, 24, font).is_none());
        assert!(scale_to_fit(400, 200, 0, 24, font).is_none());
    }

    #[test]
    fn backend_epoch_mixes_into_a_new_signature() {
        assert_eq!(next_epoch(0), 1);
        assert_eq!(next_epoch(5), 6);
        let first = mix_signature(99, next_epoch(0));
        let second = mix_signature(99, next_epoch(1));
        assert_ne!(first, second);
        assert_eq!(first, mix_signature(99, 1));
    }

    #[test]
    fn font_library_resolves_known_families_and_tracks_prewarm() {
        let mut library = FontLibrary::new();
        assert!(!library.clone().is_warmed());
        library.prewarm();
        assert!(library.clone().is_warmed());
        assert_eq!(library.clone().get("mono"), Some(FontMetrics::default()));
        assert_eq!(library.get("unknown-family"), None);
    }

    #[test]
    fn vector_argv_stays_a_pure_hook() {
        let argv = vector_raster_argv("diagram.svg", 640, VectorSource::Mermaid);
        assert_eq!(argv.first().map(String::as_str), Some("convert"));
        assert!(argv.contains(&"640x".to_string()));
    }
}
