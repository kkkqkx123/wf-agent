use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "wf-debug", about = "Offline workflow and agent debugger")]
pub struct DebuggerCli {
    #[command(subcommand)]
    pub command: DebuggerCommand,
}

#[derive(Debug, Subcommand)]
pub enum DebuggerCommand {
    Replay(ReplayArgs),
    Check(CheckArgs),
    Branches(BranchesArgs),
    Hooks(HooksArgs),
    Triggers(TriggersArgs),
    Assert(AssertArgs),
    Timeline(TimelineArgs),
    Agents(AgentsArgs),
    Compression(CompressionArgs),
    Import(ImportArgs),
    Spec(SpecArgs),
}

#[derive(Debug, clap::Args)]
pub struct ReplayArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub no_color: bool,
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub no_color: bool,
}

#[derive(Debug, clap::Args)]
pub struct BranchesArgs {
    #[arg(long)]
    pub decision: PathBuf,
    #[arg(long)]
    pub variables: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct HooksArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct TriggersArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long)]
    pub event_type: Option<String>,
    #[arg(long)]
    pub event_name: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct AssertArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct TimelineArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct AgentsArgs {
    #[arg(long)]
    pub trace: PathBuf,
    /// Builtin agent template to check against (for example
    /// `@standard/explorer`). Overrides the trace identity when set.
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct CompressionArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct ImportArgs {
    #[arg(long)]
    pub snapshot: PathBuf,
    /// Write the normalized trace here; prints to stdout when absent.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct SpecArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}
