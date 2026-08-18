//! Checkpoint debugging CLI: inspect persisted checkpoints without
//! wiring up a full runtime.
//!
//! ```text
//! wf-checkpoint --db checkpoints.db list <execution-id>
//! wf-checkpoint --db checkpoints.db dump <checkpoint-id>
//! ```
//!
//! The store is opened read-mostly; `list` prints storage metadata (id,
//! type, timestamp, chain position, blob size) and `dump` prints the full
//! checkpoint envelope as readable JSON (gzip payloads are transparently
//! decompressed).

use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use wf_checkpoint::state::CheckpointStateManager;
use wf_checkpoint::state::StorageBackedStateManager;
use wf_storage::backend::StorageBackend;
use wf_types::checkpoint::BaseCheckpointCore;

type GenericCheckpoint = BaseCheckpointCore<serde_json::Value, serde_json::Value>;

#[derive(Parser)]
#[command(
    name = "wf-checkpoint",
    about = "Checkpoint debugging CLI: list and dump persisted checkpoints"
)]
struct Cli {
    /// SQLite database path. Defaults to a scratch in-memory store (useful
    /// for smoke-testing the tool itself).
    #[arg(long, default_value = ":memory:")]
    db: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the checkpoints of one execution (newest first).
    List(ListArgs),
    /// Dump one checkpoint as readable JSON.
    Dump(DumpArgs),
}

#[derive(Args)]
struct ListArgs {
    /// Execution (entity) id whose checkpoints are listed.
    execution_id: String,
}

#[derive(Args)]
struct DumpArgs {
    /// Checkpoint id to dump.
    checkpoint_id: String,
}

async fn open_manager(db: &str) -> Result<StorageBackedStateManager<GenericCheckpoint>, String> {
    let backend = if db == ":memory:" {
        StorageBackend::new_memory()
    } else {
        StorageBackend::new_sqlite(db, "checkpoints")
            .await
            .map_err(|e| format!("failed to open '{}': {}", db, e))?
    };
    Ok(StorageBackedStateManager::new(Arc::new(backend)))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli).await {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

async fn run(cli: &Cli) -> Result<(), String> {
    let manager = open_manager(&cli.db).await?;

    match &cli.command {
        Command::List(args) => {
            let checkpoints = manager
                .list_by_entity_paged(&args.execution_id, 0, 1000)
                .await
                .map_err(|e| format!("list failed: {}", e))?;
            if checkpoints.is_empty() {
                println!("no checkpoints for execution '{}'", args.execution_id);
                return Ok(());
            }
            println!(
                "{} checkpoints for execution '{}':",
                checkpoints.len(),
                args.execution_id
            );
            for meta in &checkpoints {
                println!(
                    "  {}  type={:<5} ts={} status={} chain_pos={} blob_size={}",
                    meta.id,
                    format!("{:?}", meta.checkpoint_type).to_uppercase(),
                    meta.timestamp,
                    format!("{:?}", meta.status).to_lowercase(),
                    meta.chain_position
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    meta.blob_size
                        .map(|b| b.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
            }
        }
        Command::Dump(args) => {
            let checkpoint = manager
                .load(&args.checkpoint_id)
                .await
                .map_err(|e| format!("dump failed: {}", e))?
                .ok_or_else(|| format!("checkpoint '{}' not found", args.checkpoint_id))?;
            let pretty = serde_json::to_string_pretty(&checkpoint)
                .map_err(|e| format!("serialization failed: {}", e))?;
            println!("{}", pretty);
        }
    }

    Ok(())
}
