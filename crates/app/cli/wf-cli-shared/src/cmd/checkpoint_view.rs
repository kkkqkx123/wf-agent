//! Text views for file-checkpoint provenance commands.
//!
//! The server owns the typed views; the CLI prints the JSON shapes it
//! returns, keeping the established text layout. Every printer takes the
//! unwrapped envelope payload and writes to stdout, matching the execution
//! view helpers.

fn str_field<'a>(value: &'a serde_json::Value, name: &str) -> &'a str {
    value.get(name).and_then(|v| v.as_str()).unwrap_or("-")
}

fn i64_field(value: &serde_json::Value, name: &str) -> i64 {
    value.get(name).and_then(|v| v.as_i64()).unwrap_or(0)
}

/// Git-style short commit id: the leading seven hex characters.
fn short_id(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

/// Render an epoch timestamp; accepts seconds or milliseconds.
fn format_ts(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    let (secs, millis) = if ts > 10_000_000_000 {
        (ts / 1000, (ts % 1000) as u32)
    } else {
        (ts, 0u32)
    };
    match chrono::DateTime::from_timestamp(secs, millis * 1_000_000) {
        Some(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => "-".to_string(),
    }
}

/// Chronological version history of one file path, with rename context.
pub(crate) fn print_file_timeline(view: &serde_json::Value) {
    let original = str_field(view, "original_path");
    let total = view.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    println!("{original} ({total} versions)");
    let entries = view.get("entries").and_then(|v| v.as_array());
    for entry in entries.into_iter().flatten() {
        let path = str_field(entry, "path");
        let snapshot = str_field(entry, "snapshot_id");
        println!(
            "  {}  {}  {}  ({})",
            format_ts(i64_field(entry, "timestamp")),
            short_id(snapshot),
            path,
            str_field(entry, "source"),
        );
        if let Some(moved) = entry.get("moved_from").and_then(|v| v.as_str()) {
            println!("      renamed from {moved}");
        }
    }
    if view
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        println!("  (listing truncated)");
    }
}

/// Files with unresolved merge conflicts and their region counts.
pub(crate) fn print_conflicts(conflicts: &serde_json::Value) {
    let rows = conflicts.as_array().map(Vec::as_slice).unwrap_or_default();
    if rows.is_empty() {
        println!("no conflicts");
        return;
    }
    println!("{} conflicted file(s):", rows.len());
    for row in rows {
        let regions = row
            .get("conflicts")
            .and_then(|v| v.as_array())
            .map(Vec::len)
            .unwrap_or(0);
        println!(
            "  {}  {}  {} region(s)  [{}]",
            str_field(row, "path"),
            short_id(str_field(row, "snapshot_id")),
            regions,
            str_field(row, "partition"),
        );
    }
}

/// Reconstructed file set of an actor workspace (metadata only).
pub(crate) fn print_workspace(files: &serde_json::Value) {
    let rows = files.as_array().map(Vec::as_slice).unwrap_or_default();
    println!("{} file(s):", rows.len());
    for row in rows {
        println!(
            "  {}  {}  {}",
            str_field(row, "path"),
            short_id(str_field(row, "hash")),
            format_ts(i64_field(row, "timestamp")),
        );
    }
}

/// Workspace file tree (metadata only) with its truncation state.
pub(crate) fn print_file_tree(view: &serde_json::Value) {
    let empty = Vec::new();
    let entries = view
        .get("entries")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let total = view
        .get("total")
        .and_then(|v| v.as_u64())
        .unwrap_or(entries.len() as u64);
    println!("{} entries ({} total):", entries.len(), total);
    for entry in entries {
        println!(
            "  {}  {}  {} B  {}",
            str_field(entry, "path"),
            short_id(str_field(entry, "hash")),
            entry.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            format_ts(i64_field(entry, "timestamp")),
        );
    }
    if view
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        println!("  (listing truncated)");
    }
}

/// Staged edit sessions (edit groups), newest first, plus the page footer.
pub(crate) fn print_edit_sessions(view: &serde_json::Value) {
    // The endpoint returns a `PageView`; fall back to a bare array so the
    // printer also serves direct list payloads.
    let items = view
        .get("items")
        .and_then(|v| v.as_array())
        .or_else(|| view.as_array());
    let rows = items.map(Vec::as_slice).unwrap_or_default();
    if rows.is_empty() {
        println!("no edit sessions");
        return;
    }
    println!("{} edit session(s):", rows.len());
    for row in rows {
        let label = row.get("label").and_then(|v| v.as_str()).unwrap_or("-");
        println!(
            "  {}  {}  {}",
            str_field(row, "id"),
            label,
            format_ts(i64_field(row, "created_at")),
        );
    }
    if view
        .get("has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let limit = view.get("limit").and_then(|v| v.as_u64()).unwrap_or(0);
        let offset = view.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("  (more results; continue with --offset {})", offset + limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_id_trims_and_passes_through() {
        assert_eq!(short_id("abcdef0123456789"), "abcdef0");
        assert_eq!(short_id("abc"), "abc");
        assert_eq!(short_id(""), "");
    }

    #[test]
    fn format_ts_accepts_seconds_and_millis() {
        assert_eq!(format_ts(0), "-");
        let secs = 1_700_000_000_i64;
        assert_eq!(format_ts(secs), format_ts(secs * 1000));
    }
}
