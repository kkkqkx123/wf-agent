//! Wrapped-row to source-column mapping for precise selection and copy.
//!
//! [`crate::anchor::WrappedLineMap`] in [`crate::anchor`] resolves a wrapped row to its
//! logical line. This module goes one step further: [`WrappedColumnMap`]
//! resolves a wrapped row to the source column interval it was reflowed
//! from, so a visual selection spanning wrapped rows maps back to exact
//! source ranges for copy. Widths follow `unicode-width` display columns,
//! matching the transcript reflow rule.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// One wrapped row and the source column interval it covers.
///
/// Columns count display cells (wide glyphs occupy two), matching the
/// reflow that produced the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedInterval {
    /// Logical source line index.
    pub logical: usize,
    /// Wrapped row index within the logical line.
    pub wrapped_row: usize,
    /// First source display column covered by this wrapped row.
    pub col_start: usize,
    /// One past the last source display column covered.
    pub col_end: usize,
}

/// Wrapped-row to source-column-interval map for one logical line set.
///
/// Built alongside reflow: the caller feeds each logical line once with its
/// wrapped-row count and per-row column widths, and the map answers which
/// source interval any wrapped row came from.
#[derive(Debug, Clone, Default)]
pub struct WrappedColumnMap {
    intervals: Vec<WrappedInterval>,
}

impl WrappedColumnMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one logical line reflowed into `widths.len()` wrapped rows,
    /// where `widths[i]` is the source display-column span of wrapped row
    /// `i`. Rows stack in order, so each interval starts where the previous
    /// ended.
    pub fn push_line(&mut self, logical: usize, widths: &[usize]) {
        let mut col = 0usize;
        for (row, span) in widths.iter().enumerate() {
            let end = col + *span;
            self.intervals.push(WrappedInterval {
                logical,
                wrapped_row: row,
                col_start: col,
                col_end: end,
            });
            col = end;
        }
    }

    /// Build a map from already-reflowed display rows: `display_counts[i]`
    /// wrapped rows for logical line `i`, each spanning the full `width`
    /// columns except possibly the last row of the line.
    pub fn build(width: usize, display_counts: &[usize]) -> Self {
        let mut map = Self::new();
        for (logical, count) in display_counts.iter().enumerate() {
            let widths = vec![width; *count];
            map.push_line(logical, &widths);
        }
        map
    }

    /// Interval for wrapped row `wrapped`, if present.
    pub fn interval_for_wrapped(&self, wrapped: usize) -> Option<WrappedInterval> {
        self.intervals.get(wrapped).copied()
    }

    /// Number of wrapped rows in the map.
    pub fn wrapped_len(&self) -> usize {
        self.intervals.len()
    }

    /// Slice the source text of logical line `logical` to the display
    /// columns `col_start..col_end`, respecting grapheme boundaries so wide
    /// glyphs are never split.
    pub fn slice_columns(text: &str, col_start: usize, col_end: usize) -> String {
        let mut out = String::new();
        let mut col = 0usize;
        for grapheme in text.graphemes(true) {
            let width = grapheme.width().max(1);
            let next = col + width;
            if next > col_start && col < col_end {
                out.push_str(grapheme);
            }
            col = next;
            if col >= col_end {
                break;
            }
        }
        out
    }

    /// Copy the visual range from `anchor` to `focus` (wrapped-row units,
    /// inclusive) out of `source_lines`. Rows between the endpoints copy
    /// whole; the endpoint rows clip to their covered column intervals.
    pub fn copy_range(
        &self,
        source_lines: &[String],
        anchor: usize,
        focus: usize,
    ) -> Option<String> {
        let (from, to) = if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        };
        let first = self.interval_for_wrapped(from)?;
        let last = self.interval_for_wrapped(to)?;
        let mut out = Vec::new();
        let mut current = first.logical;
        // First partial row: from its start column to the line end when the
        // selection continues, else to the last interval end.
        let first_text = source_lines.get(first.logical)?;
        if first.logical == last.logical {
            out.push(Self::slice_columns(
                first_text,
                first.col_start,
                last.col_end,
            ));
        } else {
            out.push(Self::slice_columns(first_text, first.col_start, usize::MAX));
            current += 1;
            while current < last.logical {
                out.push(source_lines.get(current)?.clone());
                current += 1;
            }
            let last_text = source_lines.get(last.logical)?;
            out.push(Self::slice_columns(last_text, 0, last.col_end));
        }
        Some(out.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_stack_columns_in_order() {
        let mut map = WrappedColumnMap::new();
        map.push_line(0, &[10, 4]);
        map.push_line(1, &[7]);
        assert_eq!(
            map.interval_for_wrapped(0),
            Some(WrappedInterval {
                logical: 0,
                wrapped_row: 0,
                col_start: 0,
                col_end: 10,
            })
        );
        assert_eq!(
            map.interval_for_wrapped(1),
            Some(WrappedInterval {
                logical: 0,
                wrapped_row: 1,
                col_start: 10,
                col_end: 14,
            })
        );
        assert_eq!(map.interval_for_wrapped(2).map(|i| i.logical), Some(1));
        assert_eq!(map.interval_for_wrapped(3), None);
    }

    #[test]
    fn slice_keeps_wide_glyphs_whole() {
        let text = "你好世界 hello";
        // Each CJK char is 2 columns; slicing mid-glyph keeps the glyph.
        assert_eq!(WrappedColumnMap::slice_columns(text, 0, 2), "你");
        assert_eq!(WrappedColumnMap::slice_columns(text, 0, 4), "你好");
        assert_eq!(WrappedColumnMap::slice_columns(text, 2, 6), "好世");
    }

    #[test]
    fn copy_range_clips_endpoints_and_keeps_middle_whole() {
        let mut map = WrappedColumnMap::new();
        map.push_line(0, &[5, 5]);
        map.push_line(1, &[10]);
        map.push_line(2, &[10]);
        let source = vec![
            "abcdefghij".to_string(),
            "middle line".to_string(),
            "last line!".to_string(),
        ];
        let copied = map.copy_range(&source, 0, 3).expect("range");
        assert_eq!(copied, "abcdefghij\nmiddle line\nlast line!");
    }

    #[test]
    fn copy_range_within_one_line_clips_both_sides() {
        let mut map = WrappedColumnMap::new();
        map.push_line(0, &[5, 5]);
        let source = vec!["abcdefghij".to_string()];
        assert_eq!(map.copy_range(&source, 0, 1).as_deref(), Some("abcdefghij"));
        assert_eq!(map.copy_range(&source, 0, 9), None);
    }
}
