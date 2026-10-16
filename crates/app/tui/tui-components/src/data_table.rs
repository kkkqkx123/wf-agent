//! Plain-text aligned table helpers used by management screens.
//!
//! The helpers keep table width calculation deterministic and unit-testable
//! without requiring a ratatui frame.

/// Compute the width of each column from the widest cell in every row.
pub fn column_widths(rows: &[Vec<String>]) -> Vec<usize> {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    (0..width)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(String::len)
                .max()
                .unwrap_or(0)
        })
        .collect()
}

/// Format one row's cells according to precomputed column widths.
pub fn format_row(row: &[String], widths: &[usize]) -> String {
    row.iter()
        .enumerate()
        .map(|(index, cell)| {
            let width = widths.get(index).copied().unwrap_or(0);
            format!("{cell:<width$}")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// Render an aligned textual table. The first row is treated as the header.
pub fn format_table(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let widths = column_widths(rows);
    let mut out = String::new();
    out.push_str(&format_row(&rows[0], &widths));
    out.push('\n');
    out.push_str(
        &widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("--"),
    );
    out.push('\n');
    for row in rows.iter().skip(1) {
        out.push_str(&format_row(row, &widths));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_widths_use_max_cell_width() {
        let rows = vec![
            vec!["Name".into(), "Status".into()],
            vec!["demo".into(), "ok".into()],
        ];
        assert_eq!(column_widths(&rows), vec![4, 6]);
    }

    #[test]
    fn format_table_aligns_header_and_rows() {
        let rows = vec![
            vec!["Name".into(), "Value".into()],
            vec!["a".into(), "1".into()],
            vec!["longer".into(), "2".into()],
        ];
        let actual = format_table(&rows);
        assert_eq!(actual.lines().count(), 4);
        assert!(actual.starts_with("Name    Value"));
    }

    #[test]
    fn format_table_handles_empty_input() {
        assert_eq!(format_table(&[]), "");
    }
}
