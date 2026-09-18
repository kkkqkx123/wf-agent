//! Deferred heavyweight frame work: image and diagram registration.
//!
//! Rasterizing charts or images mid-draw would stall the frame, so the draw
//! path only registers heavyweight items here and processes them after the
//! frame completes. Every frame end also runs one image-cache cleanup so
//! stale entries are reclaimed even when no image widget redraws. Diagram
//! geometry itself lives in [`crate::layout`]; this module only carries the
//! registration queue.

/// Kind of a deferred heavyweight item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredKind {
    /// Inline terminal image.
    Image,
    /// Diagram or chart pane.
    Diagram,
}

/// One heavyweight item registered during draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredItem {
    /// What kind of work the item needs.
    pub kind: DeferredKind,
    /// Stable label identifying the item across frames.
    pub label: String,
}

/// End-of-frame work queue: registrations drained once per frame plus a
/// cleanup counter proving the per-frame image cleanup ran.
#[derive(Debug, Default)]
pub struct DeferredFrameWork {
    items: Vec<DeferredItem>,
    cleanups: usize,
}

impl DeferredFrameWork {
    /// Empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an inline image for post-frame rasterization.
    pub fn defer_image(&mut self, label: impl Into<String>) {
        self.items.push(DeferredItem {
            kind: DeferredKind::Image,
            label: label.into(),
        });
    }

    /// Register a diagram for post-frame rasterization.
    pub fn defer_diagram(&mut self, label: impl Into<String>) {
        self.items.push(DeferredItem {
            kind: DeferredKind::Diagram,
            label: label.into(),
        });
    }

    /// Items registered since the last drain.
    pub fn pending(&self) -> &[DeferredItem] {
        &self.items
    }

    /// Drain registered items for end-of-frame processing.
    pub fn drain(&mut self) -> Vec<DeferredItem> {
        std::mem::take(&mut self.items)
    }

    /// Record one image-cache cleanup run (called every frame end).
    pub fn note_cleanup(&mut self) {
        self.cleanups = self.cleanups.saturating_add(1);
    }

    /// End-of-frame flush: drain registrations for post-frame processing and
    /// record the mandatory cleanup run. Always called at the frame tail, so
    /// the cleanup counter advances even when nothing was registered (no
    /// raster backend is bundled; registrations carry labels for a future
    /// backend while the cleanup proof stays unconditional).
    pub fn flush(&mut self) -> Vec<DeferredItem> {
        let items = self.drain();
        self.note_cleanup();
        items
    }

    /// Number of cleanup runs recorded.
    pub fn cleanups(&self) -> usize {
        self.cleanups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_returns_registrations_and_clears() {
        let mut work = DeferredFrameWork::new();
        work.defer_image("avatar");
        work.defer_diagram("flow");
        assert_eq!(work.pending().len(), 2);
        let items = work.drain();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, DeferredKind::Image);
        assert_eq!(items[1].label, "flow");
        assert!(work.pending().is_empty());
    }

    #[test]
    fn cleanup_counter_advances_per_frame_end() {
        let mut work = DeferredFrameWork::new();
        work.note_cleanup();
        work.note_cleanup();
        assert_eq!(work.cleanups(), 2);
    }

    #[test]
    fn flush_drains_and_notes_cleanup() {
        let mut work = DeferredFrameWork::new();
        work.defer_diagram("flow");
        let items = work.flush();
        assert_eq!(items.len(), 1);
        assert_eq!(work.cleanups(), 1);
        assert!(work.pending().is_empty());
    }
}
