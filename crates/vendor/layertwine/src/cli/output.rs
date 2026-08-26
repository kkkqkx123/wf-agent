use crate::api::types::*;
use crate::core::delta::Delta;
use crate::core::partition::Partition;
use crate::core::snapshot::Snapshot;
use crate::core::types::DiffOp;
use crate::core::types::LineDiff;

/// Output format mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Plain,
    Json,
}

/// Format status from ApiService response
pub fn format_status_response(resp: &StatusResponse, format: OutputFormat) -> String {
    match format {
        OutputFormat::Plain => {
            if resp.partitions.is_empty() {
                return "No partitions found. Run 'layertwine init' to initialize.".into();
            }
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("{:-<72}", ""));
            lines.push(format!(
                "{:<16} {:<24} {:<20} History",
                "Layer", "Partition", "Current Snapshot"
            ));
            lines.push(format!("{:-<72}", ""));
            for entry in &resp.partitions {
                let short_hash = if entry.current_snapshot.len() > 12 {
                    &entry.current_snapshot[..12]
                } else {
                    &entry.current_snapshot
                };
                lines.push(format!(
                    "{:<16} {:<24} {:<20} {} snapshots",
                    entry.layer, entry.name, short_hash, entry.history_len
                ));
            }
            lines.push(format!("{:-<72}", ""));
            lines.join("\n")
        }
        OutputFormat::Json => serde_json::to_string_pretty(resp).unwrap(),
    }
}

/// Format checkpoint history from ApiService response
pub fn format_log_response(resp: &LogResponse, format: OutputFormat) -> String {
    match format {
        OutputFormat::Plain => {
            if resp.checkpoints.is_empty() {
                return "No checkpoints found.".into();
            }
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("{:-<100}", ""));
            lines.push(format!(
                "{:<20} {:<16} {:<12} {:<12} {:<30}",
                "Checkpoint ID", "Author", "Parents", "Snapshots", "Message"
            ));
            lines.push(format!("{:-<100}", ""));
            for cp in resp.checkpoints.iter().rev() {
                let short_id = &cp.id[..12];
                let short_msg = if cp.message.len() > 27 {
                    format!("{}...", &cp.message[..27])
                } else {
                    cp.message.clone()
                };
                let git_tag = if cp.git_anchor.is_some() {
                    " [git]"
                } else {
                    ""
                };
                lines.push(format!(
                    "{:<20} {:<16} {:<12} {:<12} {:<30}",
                    format!("{}{}", short_id, git_tag),
                    cp.author,
                    cp.parents.len(),
                    cp.snapshots.len(),
                    short_msg,
                ));
            }
            lines.push(format!("{:-<100}", ""));
            lines.push(format!("Total: {} checkpoint(s)", resp.checkpoints.len()));
            lines.join("\n")
        }
        OutputFormat::Json => serde_json::to_string_pretty(resp).unwrap(),
    }
}

/// Format branch list from ApiService response
pub fn format_branches_response(resp: &BranchListResponse, format: OutputFormat) -> String {
    match format {
        OutputFormat::Plain => {
            if resp.branches.is_empty() {
                return "No branches found.".into();
            }
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("{:-<60}", ""));
            lines.push(format!("{:<24} {:<20} Updated", "Branch", "Head"));
            lines.push(format!("{:-<60}", ""));
            for b in &resp.branches {
                let marker = if b.is_current { "* " } else { "  " };
                let short_head = &b.head[..12];
                lines.push(format!(
                    "{}{:<22} {:<20} {}",
                    marker, b.name, short_head, b.updated_at
                ));
            }
            lines.push(format!("{:-<60}", ""));
            lines.join("\n")
        }
        OutputFormat::Json => serde_json::to_string_pretty(resp).unwrap(),
    }
}

/// Format delta diff display (similar to `git diff`)
pub fn format_diff(delta: &Delta, format: OutputFormat) -> String {
    match format {
        OutputFormat::Plain => {
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!(
                "diff --git a/{} b/{}",
                delta.file.path_str(),
                delta.file.path_str()
            ));
            let summary = delta.summary();
            lines.push(format!(
                "--- a/{}\n+++ b/{}",
                delta.file.path_str(),
                delta.file.path_str()
            ));
            lines.push(format!(
                "@@ ... @@ ({} inserts, {} deletes, {} replaces)",
                summary.inserts, summary.deletes, summary.replaces
            ));
            lines.push(format_line_diff(&delta.diff));
            lines.join("\n")
        }
        OutputFormat::Json => {
            let summary = delta.summary();
            let json = serde_json::json!({
                "file": delta.file.path_str(),
                "source": format!("{:?}", delta.source),
                "timestamp": delta.timestamp,
                "summary": {
                    "inserts": summary.inserts,
                    "deletes": summary.deletes,
                    "replaces": summary.replaces,
                    "hunks": summary.total_hunks,
                },
                "diff": format!("{:?}", delta.diff),
            });
            serde_json::to_string_pretty(&json).unwrap()
        }
    }
}

/// Format show response (unified diff display)
pub fn format_show_response(resp: &ShowResponse, format: OutputFormat) -> String {
    match format {
        OutputFormat::Plain => {
            let mut lines: Vec<String> = Vec::new();
            for diff in &resp.diffs {
                lines.push(format!("--- a/{}", diff.file_path));
                lines.push(format!("+++ b/{}", diff.file_path));
                lines.push(diff.unified_diff.clone());
            }
            lines.join("\n")
        }
        OutputFormat::Json => serde_json::to_string_pretty(resp).unwrap(),
    }
}

fn format_line_diff(diff: &LineDiff) -> String {
    let mut lines: Vec<String> = Vec::new();
    for hunk in &diff.hunks {
        lines.push(format!(
            "@@ -{},{} +{},{} @@",
            hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
        ));
        for op in &hunk.ops {
            match op {
                DiffOp::Equal { count } => {
                    for _ in 0..*count {
                        lines.push("  ".into());
                    }
                }
                DiffOp::Delete { count, .. } => {
                    for _ in 0..*count {
                        lines.push("- ".into());
                    }
                }
                DiffOp::Insert {
                    lines: inserted, ..
                } => {
                    for line in inserted {
                        lines.push(format!("+{}", line));
                    }
                }
                DiffOp::Replace {
                    old_count,
                    lines: inserted,
                    ..
                } => {
                    for _ in 0..*old_count {
                        lines.push("- ".into());
                    }
                    for line in inserted {
                        lines.push(format!("+{}", line));
                    }
                }
            }
        }
    }
    lines.join("\n")
}

/// Format snapshot summary
pub fn format_snapshot(snapshot: &Snapshot, deltas: &[Delta], format: OutputFormat) -> String {
    let file_path = deltas
        .last()
        .map(|d| d.file.path_str())
        .unwrap_or_else(|| snapshot.file.path_str());

    match format {
        OutputFormat::Plain => {
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("Snapshot: {}", snapshot.id.to_hex()));
            lines.push(format!("  File:     {}", file_path));
            lines.push(format!("  Type:     {}", snapshot.partition_type));
            lines.push(format!("  Deltas:   {}", snapshot.deltas.len()));
            lines.push(format!("  Parents:  {}", snapshot.parents.len()));
            lines.push(format!(
                "  Created:  {}",
                chrono::DateTime::from_timestamp_millis(snapshot.created_at)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string())
            ));
            if !snapshot.parents.is_empty() {
                lines.push("  Parent IDs:".into());
                for (i, pid) in snapshot.parents.iter().enumerate() {
                    lines.push(format!("    [{}] {}", i, pid.to_hex()));
                }
            }
            lines.join("\n")
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "id": snapshot.id.to_hex(),
                "file": file_path,
                "partition_type": snapshot.partition_type,
                "delta_count": snapshot.deltas.len(),
                "parent_count": snapshot.parents.len(),
                "parents": snapshot.parents.iter().map(|p| p.to_hex()).collect::<Vec<_>>(),
                "created_at": snapshot.created_at,
            });
            serde_json::to_string_pretty(&json).unwrap()
        }
    }
}

/// Format a partition's details
pub fn format_partition(partition: &Partition, format: OutputFormat) -> String {
    match format {
        OutputFormat::Plain => {
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("Partition: {}", partition.name));
            lines.push(format!("  ID:      {}", partition.id));
            lines.push(format!("  Type:    {:?}", partition.partition_type));
            lines.push(format!(
                "  Current: {}",
                &partition.current_snapshot.to_hex()[..12]
            ));
            lines.push(format!("  History: {} snapshots", partition.history.len()));
            lines.join("\n")
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "id": partition.id.to_string(),
                "name": partition.name,
                "partition_type": format!("{:?}", partition.partition_type),
                "current_snapshot": partition.current_snapshot.to_hex(),
                "history_len": partition.history.len(),
            });
            serde_json::to_string_pretty(&json).unwrap()
        }
    }
}

/// Format backup snapshot details
pub fn format_backup_snapshot(
    bs: &crate::backup::backup_snapshot::BackupSnapshot,
    format: OutputFormat,
) -> String {
    let file_path = bs
        .deltas
        .last()
        .map(|d| d.file.path_str())
        .unwrap_or_else(|| bs.file.path_str());

    match format {
        OutputFormat::Plain => {
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("Backup: {}", bs.id.to_hex()));
            lines.push(format!("  Source:  {}", bs.source_snapshot.to_hex()));
            lines.push(format!("  File:    {}", file_path));
            lines.push(format!("  Deltas:  {}", bs.deltas.len()));
            if let Some(ref label) = bs.label {
                lines.push(format!("  Label:   {}", label));
            }
            lines.push(format!(
                "  Time:    {}",
                chrono::DateTime::from_timestamp_millis(bs.backed_at)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string())
            ));
            lines.join("\n")
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "id": bs.id.to_hex(),
                "source_snapshot": bs.source_snapshot.to_hex(),
                "file": file_path,
                "deltas_count": bs.deltas.len(),
                "label": bs.label,
                "backed_at": bs.backed_at,
                "agent_id": bs.agent_id,
                "source_type": bs.source_type,
            });
            serde_json::to_string_pretty(&json).unwrap()
        }
    }
}

/// Print a progress message to stderr
pub fn print_progress(message: &str) {
    eprint!("  {} ... ", message);
    std::io::Write::flush(&mut std::io::stderr()).ok();
}

/// Print a done message to stderr (completing a progress indicator)
pub fn print_done() {
    eprintln!("done");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_enum() {
        assert_ne!(OutputFormat::Plain as u8, OutputFormat::Json as u8);
    }
}
