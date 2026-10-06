//! Observability subcommand arguments (audit, events, metrics, analysis).

use clap::Subcommand;

/// Audit subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum AuditSub {
    /// Audit summary.
    Summary {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Full audit report.
    Report {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Audit timeline.
    Timeline {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Iteration audit.
    Iterations {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Tool calls audit.
    ToolCalls {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// LLM calls audit.
    LlmCalls {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Node executions audit.
    NodeExecutions {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
}

/// Event subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum EventSub {
    /// List events.
    List {
        /// Filter by execution id.
        #[arg(long, value_name = "ID")]
        execution: Option<String>,
        /// Filter by workflow id.
        #[arg(long, value_name = "ID")]
        workflow: Option<String>,
        /// Filter by agent loop id.
        #[arg(long = "agent-loop", value_name = "ID")]
        agent_loop: Option<String>,
        /// Filter by event types (comma-separated).
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,
        /// Limit.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Event statistics.
    Stats,
    /// Execution timeline.
    Timeline {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Follow events (streaming).
    Follow {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Filter by event types (comma-separated).
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,
        /// Also include workflow id filter.
        #[arg(long, value_name = "ID")]
        workflow: Option<String>,
        /// Polling interval in milliseconds (fallback when subscription unavailable).
        #[arg(long, value_name = "MS", default_value_t = 500)]
        interval: u64,
        /// Only fetch once (no streaming).
        #[arg(long)]
        once: bool,
    },
}

/// Metrics subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum MetricsSub {
    /// Show metrics snapshot.
    Show {
        /// Export format (json or prometheus).
        #[arg(long, value_name = "FORMAT")]
        export: Option<String>,
    },
    /// Export metrics (alias for show --export).
    Export {
        /// Export format (json or prometheus).
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: String,
    },
}

/// Analysis subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum AnalysisSub {
    /// Performance analysis of an execution.
    Performance {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Bottleneck analysis.
    Bottleneck {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Error analysis.
    Errors {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Include error chain.
        #[arg(long)]
        chain: bool,
        /// Include root cause.
        #[arg(long)]
        root_cause: bool,
        /// Include recovery proposals.
        #[arg(long)]
        recovery: bool,
    },
    /// Compare two executions.
    Compare {
        /// Baseline execution id.
        #[arg(value_name = "BASELINE")]
        baseline: String,
        /// Compared execution id.
        #[arg(value_name = "COMPARED")]
        compared: String,
    },
    /// Progress of an execution.
    Progress {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
}
