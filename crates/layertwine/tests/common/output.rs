//! Formatted output utilities for E2E tests
#![allow(dead_code, unused_imports, unused_variables)]

use layertwine::api::StatusResponse;
use layertwine::core::partition::Partition;
use layertwine::core::types::LayerType;
use layertwine::core::types::SnapshotId;
use std::fmt;

/// Formatted layer state output
#[derive(Debug)]
pub struct LayerStateOutput {
    pub layer_name: String,
    pub partitions: Vec<PartitionOutput>,
    pub total_snapshots: usize,
}

/// Formatted partition output
#[derive(Debug)]
pub struct PartitionOutput {
    pub name: String,
    pub current_snapshot: String,
    pub history_depth: usize,
    pub preview: String,
}

/// Formatted diff output
#[derive(Debug)]
pub struct DiffOutput {
    pub file_path: String,
    pub unified_diff: String,
    pub stats: DiffStats,
    pub conflicts: Vec<String>,
}

/// Diff statistics
#[derive(Debug)]
pub struct DiffStats {
    pub inserts: usize,
    pub deletes: usize,
    pub replaces: usize,
}

impl fmt::Display for LayerStateOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Layer: {}", self.layer_name)?;
        writeln!(f, "  Total snapshots: {}", self.total_snapshots)?;
        writeln!(f, "  Partitions: {}", self.partitions.len())?;

        for partition in &self.partitions {
            writeln!(f)?;
            writeln!(f, "    - {}", partition.name)?;
            writeln!(
                f,
                "      Current snapshot: {}",
                truncate_id(&partition.current_snapshot)
            )?;
            writeln!(f, "      History depth: {}", partition.history_depth)?;
            writeln!(f, "      Preview: {}", preview_lines(&partition.preview))?;
        }

        Ok(())
    }
}

impl fmt::Display for DiffOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "File: {}", self.file_path)?;
        writeln!(f)?;
        writeln!(f, "{}", self.unified_diff)?;
        writeln!(f)?;
        writeln!(
            f,
            "Stats: {} insert, {} delete, {} replace",
            self.stats.inserts, self.stats.deletes, self.stats.replaces
        )?;

        if !self.conflicts.is_empty() {
            writeln!(f)?;
            writeln!(f, "Conflicts: {}", self.conflicts.len())?;
            for (i, conflict) in self.conflicts.iter().enumerate() {
                writeln!(f, "  Conflict {}:", i + 1)?;
                for line in conflict.lines() {
                    writeln!(f, "    {}", line)?;
                }
            }
        }

        Ok(())
    }
}

impl fmt::Display for DiffStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} insert, {} delete, {} replace",
            self.inserts, self.deletes, self.replaces
        )
    }
}

/// Print test header
pub fn print_test_header(name: &str) {
    let separator = "=".repeat(80);
    println!();
    println!("{}", separator);
    println!("E2E Test: {}", name);
    println!("{}", separator);
    println!();
}

/// Print section header
pub fn print_section_header(title: &str) {
    let separator = "-".repeat(80);
    println!();
    println!("{}", separator);
    println!("{}", title);
    println!("{}", separator);
    println!();
}

/// Print info message
pub fn print_info(message: &str) {
    println!("[INFO] {}", message);
}

/// Print success message
pub fn print_success(message: &str) {
    println!("[OK] {}", message);
}

/// Print error message
pub fn print_error(message: &str) {
    println!("[ERROR] {}", message);
}

/// Print warning message
pub fn print_warning(message: &str) {
    println!("[WARN] {}", message);
}

/// Print test result
pub fn print_test_result(passed: bool, message: &str, duration: Option<std::time::Duration>) {
    let separator = "=".repeat(80);
    println!();
    println!("{}", separator);

    if passed {
        print!("[PASS] ");
    } else {
        print!("[FAIL] ");
    }

    print!("{}", message);

    if let Some(d) = duration {
        print!(" ({:.2}s)", d.as_secs_f64());
    }

    println!();
    println!("{}", separator);
}

/// Print separator line
pub fn print_separator() {
    println!("{}", "-".repeat(80));
}

/// Print formatted layer state from status response
pub fn print_layer_state_from_status(status: &StatusResponse) {
    print_section_header("Current Layer State");

    // Group partitions by layer
    let mut layers: std::collections::HashMap<String, Vec<&layertwine::api::PartitionInfo>> =
        std::collections::HashMap::new();

    for partition in &status.partitions {
        layers
            .entry(partition.layer.clone())
            .or_default()
            .push(partition);
    }

    // Define layer order
    let layer_order = vec![
        "manual_edit",
        "agent_edit",
        "approval",
        "integrated",
        "unified",
        "staged",
    ];

    for layer_name in layer_order {
        if let Some(partitions) = layers.get(layer_name) {
            println!("Layer: {}", layer_name);
            println!("  Partitions: {}", partitions.len());

            for partition in partitions {
                println!();
                println!("    - {}", partition.name);
                println!(
                    "      Current snapshot: {}",
                    truncate_id(&partition.current_snapshot)
                );
                println!("      History depth: {}", partition.history_len);
            }

            println!();
        }
    }
}

/// Print formatted diff output
pub fn print_diff_output(diff: &DiffOutput) {
    println!("{}", diff);
}

/// Print checkpoint log
pub fn print_checkpoint_log(log_entries: &[String]) {
    print_section_header("Checkpoint Log");

    if log_entries.is_empty() {
        println!("No checkpoints found.");
    } else {
        for (i, entry) in log_entries.iter().enumerate() {
            println!("  {}. {}", i + 1, entry);
        }
    }

    println!();
}

/// Print empty state
pub fn print_empty_state() {
    print_section_header("Current Layer State");
    println!("No partitions found.");
    println!();
}

/// Format partitions as LayerStateOutput
pub fn format_partitions(partitions: &[Partition], layer_type: LayerType) -> LayerStateOutput {
    let layer_name = layer_type.name().to_string();

    let partition_outputs: Vec<PartitionOutput> = partitions
        .iter()
        .map(|p| {
            let preview = format_partition_preview(p);
            PartitionOutput {
                name: p.name.clone(),
                current_snapshot: p.current_snapshot.to_hex(),
                history_depth: p.history.len(),
                preview,
            }
        })
        .collect();

    let total_snapshots = partition_outputs.len();

    LayerStateOutput {
        layer_name,
        partitions: partition_outputs,
        total_snapshots,
    }
}

/// Format partition preview
fn format_partition_preview(partition: &Partition) -> String {
    format!(
        "(snapshot: {})",
        truncate_id(&partition.current_snapshot.to_hex())
    )
}

/// Truncate ID to readable format
fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}...", &id[..12])
    } else {
        id.to_string()
    }
}

/// Preview lines with ellipsis if too long
fn preview_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    if lines.is_empty() {
        "(empty)".to_string()
    } else if lines.len() <= 3 {
        text.lines().take(3).collect::<Vec<_>>().join("\\n")
    } else {
        format!("{}\\n...\\n{}", lines[0], lines.last().unwrap())
    }
}

/// Create LayerStateOutput from all partitions
pub fn create_layer_state_output(
    all_partitions: &[Partition],
    layer_type: LayerType,
) -> LayerStateOutput {
    let layer_partitions: Vec<Partition> = all_partitions
        .iter()
        .filter(|p| p.partition_type.to_layer() == layer_type)
        .cloned()
        .collect();

    format_partitions(&layer_partitions, layer_type)
}

/// Print all layer states
pub fn print_all_layer_states(all_partitions: &[Partition]) {
    print_section_header("Current Layer State");

    let layer_order = vec![
        LayerType::ManualEdit,
        LayerType::AgentEdit,
        LayerType::Approval,
        LayerType::Integrated,
        LayerType::Unified,
        LayerType::Staged,
    ];

    for layer_type in layer_order {
        let state = create_layer_state_output(all_partitions, layer_type.clone());
        if !state.partitions.is_empty() {
            println!("{}", state);
            println!();
        }
    }

    if all_partitions.is_empty() {
        println!("No partitions found.");
        println!();
    }
}

/// Print file content preview
pub fn print_file_content(content: &str, max_lines: usize) {
    let lines: Vec<&str> = content.lines().collect();

    println!("File content ({} lines):", lines.len());
    println!();

    for (i, line) in lines.iter().enumerate() {
        if i >= max_lines {
            println!("... ({} more lines)", lines.len() - max_lines);
            break;
        }
        println!("  {:3}: {}", i + 1, line);
    }

    if lines.is_empty() {
        println!("  (empty)");
    }

    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_id() {
        assert_eq!(truncate_id("123456789012"), "123456789012");
        assert_eq!(truncate_id("1234567890123456"), "123456789012...");
    }

    #[test]
    fn test_preview_lines() {
        assert_eq!(
            preview_lines("line1\nline2\nline3"),
            "line1\\nline2\\nline3"
        );
        assert_eq!(preview_lines(""), "(empty)");
        assert!(preview_lines("line1\nline2\nline3\nline4").contains("..."));
    }
}
