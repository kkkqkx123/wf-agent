//! Width measurement abstraction for content layout.
//!
//! Layout code measures display columns through [`WidthMeasure`] instead of
//! reading terminal widths directly, so the wrapping algorithm stays correct
//! when a second frontend reuses it with different font metrics. The
//! terminal implementation ([`ColumnWidth`]) preserves the existing
//! Unicode display-column semantics.

/// Display-width measurement for layout.
pub trait WidthMeasure {
    /// Display columns occupied by `ch` (at least 1).
    fn cell_width(&self, ch: char) -> usize;

    /// Display columns occupied by `text`.
    fn text_width(&self, text: &str) -> usize {
        text.chars().map(|ch| self.cell_width(ch)).sum()
    }
}

/// Terminal column measurement using Unicode display width.
#[derive(Debug, Clone, Copy, Default)]
pub struct ColumnWidth;

impl WidthMeasure for ColumnWidth {
    fn cell_width(&self, ch: char) -> usize {
        column_cell_width(ch)
    }
}

/// Free-function column width sharing one implementation with [`ColumnWidth`],
/// so neutral-document wrapping can take it as a plain function pointer
/// without naming a concrete type.
pub fn column_cell_width(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch)
        .unwrap_or(1)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_chars_take_one_column() {
        let measure = ColumnWidth;
        assert_eq!(measure.cell_width('a'), 1);
        assert_eq!(measure.text_width("hello"), 5);
    }

    #[test]
    fn cjk_chars_take_two_columns() {
        let measure = ColumnWidth;
        assert_eq!(measure.cell_width('中'), 2);
        assert_eq!(measure.text_width("a中"), 3);
    }

    #[test]
    fn control_chars_fall_back_to_one_column() {
        let measure = ColumnWidth;
        assert_eq!(measure.cell_width('\u{0}'), 1);
    }
}
