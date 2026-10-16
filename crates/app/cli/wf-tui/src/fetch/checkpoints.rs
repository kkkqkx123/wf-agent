//! Checkpoint list fetcher and checkpoint detail/chain formatters.

use wf_client::WfClient;

use crate::error::CliResult;
use crate::screens::{CheckpointRow, ScreenData};

use super::common::{format_ts, page_of};

pub(super) async fn fetch_checkpoints(client: &WfClient) -> CliResult<(ScreenData, bool)> {
    let data: serde_json::Value = client.get_json("/api/v1/checkpoints").await?;
    let (items, has_more) = page_of(&data);
    let rows = items
        .into_iter()
        .filter_map(|c| {
            Some(CheckpointRow {
                id: c.get("id")?.as_str()?.to_string(),
                entity: c
                    .get("entity_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                timestamp: format_ts(c.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0)),
                entity_type: c
                    .get("entity_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                checkpoint_type: c
                    .get("checkpoint_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: c
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    Ok((ScreenData::Checkpoints(rows), has_more))
}

/// Render one execution checkpoint record as human-readable detail lines.
pub fn format_checkpoint_detail(checkpoint: &wf_types::storage::checkpoint::Checkpoint) -> String {
    let mut out = vec![
        format!("id: {}", checkpoint.id),
        format!(
            "entity: {} ({})",
            checkpoint.entity_id, checkpoint.entity_type
        ),
        format!(
            "type: {:?} · status: {:?}",
            checkpoint.checkpoint_type, checkpoint.status
        ),
        format!("timestamp: {}", format_ts(checkpoint.timestamp)),
    ];
    if let Some(position) = checkpoint.chain_position {
        out.push(format!("chain position: {position}"));
    }
    if let Some(size) = checkpoint.blob_size {
        out.push(format!("blob size: {size} bytes"));
    }
    if let Some(previous) = checkpoint.previous_checkpoint_id.as_ref() {
        out.push(format!("previous: {previous}"));
    }
    if let Some(base) = checkpoint.base_checkpoint_id.as_ref() {
        out.push(format!("base: {base}"));
    }
    if let Some(root) = checkpoint.chain_root_id.as_ref() {
        out.push(format!("chain root: {root}"));
    }
    if let Some(tags) = checkpoint.tags.as_ref() {
        out.push(format!("tags: {}", tags.join(", ")));
    }
    out.join("\n")
}

/// Render a checkpoint chain analysis as one line per checkpoint plus the
/// transition summary, for the read-only chain viewer.
pub fn format_checkpoint_chain(chain: &serde_json::Value) -> String {
    let execution_id = chain
        .get("execution_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let mut out = vec![
        format!("execution: {execution_id}"),
        format!(
            "checkpoints: {} · total elapsed: {}ms",
            chain
                .get("checkpoint_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            chain
                .get("total_elapsed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        ),
    ];
    for checkpoint in chain
        .get("checkpoints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        out.push(format!(
            "{} · {} · {} · {}",
            checkpoint.get("id").and_then(|v| v.as_str()).unwrap_or("?"),
            checkpoint
                .get("entity_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            checkpoint
                .get("checkpoint_type")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            checkpoint
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ));
    }
    for transition in chain
        .get("transitions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        out.push(format!(
            "{} -> {} · {}ms · {}",
            transition
                .get("from_checkpoint_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            transition
                .get("to_checkpoint_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
            transition
                .get("elapsed")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            transition
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        ));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_checkpoint() -> wf_types::storage::checkpoint::Checkpoint {
        wf_types::storage::checkpoint::Checkpoint {
            id: "cp-1".to_string(),
            entity_type: "agent_loop".to_string(),
            entity_id: "loop-1".to_string(),
            parent_entity_id: None,
            checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
            timestamp: 1_700_000_000,
            status: wf_types::checkpoint::CheckpointStatus::Completed,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: Some(2),
            blob_size: Some(128),
            tags: Some(vec!["t".to_string()]),
            custom_fields: None,
        }
    }

    #[test]
    fn checkpoint_detail_renders_identity_and_chain_links() {
        let text = format_checkpoint_detail(&sample_checkpoint());
        assert!(text.contains("cp-1"));
        assert!(text.contains("loop-1"));
        assert!(text.contains("agent_loop"));
        assert!(text.contains("chain position: 2"));
        assert!(text.contains("blob size: 128 bytes"));
    }

    #[test]
    fn checkpoint_chain_renders_counts_and_transitions() {
        let chain = serde_json::json!({
            "execution_id": "loop-1",
            "checkpoints": [{"id": "cp-1", "entity_id": "loop-1"}],
            "transitions": [],
            "total_elapsed": 0,
            "checkpoint_count": 1,
        });
        let text = format_checkpoint_chain(&chain);
        assert!(text.contains("execution: loop-1"));
        assert!(text.contains("cp-1"));
    }
}
