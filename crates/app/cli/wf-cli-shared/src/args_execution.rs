//! Execution subcommand arguments (`ExecutionSub`).

use clap::Subcommand;

/// Execution subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ExecutionSub {
    /// List executions.
    List {
        /// Filter by status (running, paused, completed, failed).
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Filter by workflow id.
        #[arg(long, value_name = "WORKFLOW")]
        workflow: Option<String>,
        /// Filter by agent definition id (agent executions only;
        /// cannot be combined with --workflow).
        #[arg(long, value_name = "AGENT")]
        agent: Option<String>,
        /// Merge workflow and agent runs into one newest-first listing.
        #[arg(long)]
        unified: bool,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into the result set.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
        /// Sort order by start time (asc or desc, default desc).
        #[arg(long, value_name = "ORDER", value_parser = ["asc","desc","ASC","DESC"])]
        order: Option<String>,
    },
    /// Show a single execution summary.
    Show {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Include timeline.
        #[arg(long)]
        timeline: bool,
        /// Include iterations.
        #[arg(long)]
        iterations: bool,
        /// Include variables.
        #[arg(long)]
        variables: bool,
        /// Include context evolution.
        #[arg(long = "context-evolution")]
        context_evolution: bool,
    },
    /// Run a workflow execution.
    Run {
        /// Workflow id to execute.
        #[arg(long, value_name = "ID")]
        workflow: String,
        /// Workflow input as JSON.
        #[arg(long, value_name = "JSON")]
        input: Option<String>,
        /// Return the execution id immediately and wait quietly for the
        /// terminal result instead of streaming progress. The CLI process
        /// still owns the runtime; true detach needs the server background
        /// endpoint (`--remote` against a running server).
        #[arg(long)]
        background: bool,
        /// Stream execution events to stdout while the run proceeds.
        #[arg(long)]
        stream: bool,
    },
    /// Watch an execution until it reaches a terminal status.
    Watch {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Poll interval in milliseconds.
        #[arg(long, value_name = "MS", default_value_t = 1000)]
        interval: u64,
        /// Print one status line and exit instead of watching.
        #[arg(long)]
        once: bool,
    },
    /// Show execution logs (lifecycle events as uniform log entries).
    Logs {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Keep tailing new entries until the execution ends or Ctrl-C.
        #[arg(long)]
        follow: bool,
        /// Maximum number of entries per page.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Only these event types (comma separated).
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,
        /// Poll interval in milliseconds when following.
        #[arg(long, value_name = "MS", default_value_t = 1000)]
        interval: u64,
    },
    /// Query status of an execution.
    Status {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Cancel a running execution.
    Cancel {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Optional cancel reason.
        #[arg(long, value_name = "REASON")]
        reason: Option<String>,
    },
    /// Pause a running execution.
    Pause {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Optional pause reason.
        #[arg(long, value_name = "REASON")]
        reason: Option<String>,
    },
    /// Resume a paused execution.
    Resume {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Optional resume reason.
        #[arg(long, value_name = "REASON")]
        reason: Option<String>,
    },
    /// Inspect execution state details.
    Inspect {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Include variables.
        #[arg(long)]
        variables: bool,
        /// Include status transitions.
        #[arg(long)]
        transitions: bool,
        /// Include context evolution.
        #[arg(long)]
        context: bool,
        /// Include call stack.
        #[arg(long)]
        call_stack: bool,
        /// Include variable history (requires --var-name).
        #[arg(long = "variable-history")]
        variable_history: bool,
        /// Variable name for --variable-history.
        #[arg(long = "var-name", value_name = "NAME")]
        var_name: Option<String>,
        /// Include context transitions.
        #[arg(long = "context-transitions")]
        context_transitions: bool,
        /// Include node transitions.
        #[arg(long = "node-transitions")]
        node_transitions: bool,
        /// Include memory usage.
        #[arg(long)]
        memory: bool,
    },
    /// Show where an execution sits in the parent/child tree.
    Hierarchy {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show every execution below a root, indented by depth.
    Subtree {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Maximum number of nodes in this page.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Opaque cursor from a previous page.
        #[arg(long, value_name = "CURSOR")]
        cursor: Option<String>,
    },
    /// Show everything an execution recorded, grouped by section.
    History {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Sections to load (timeline, nodes, iterations, variables, context,
        /// transitions); all sections when omitted.
        #[arg(long, value_name = "SECTIONS")]
        include: Option<String>,
    },
    /// Performance profile of an execution.
    Performance {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Bottleneck analysis of an execution.
    Bottleneck {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Error analysis of an execution.
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
        /// Include recovery proposal.
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
    /// Execution progress.
    Progress {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Execution state at iteration (time-travel).
    State {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Iteration number.
        #[arg(long, value_name = "N")]
        at_iteration: Option<u64>,
        /// Variable name to show history for.
        #[arg(long, value_name = "NAME", conflicts_with = "most_changed")]
        variable: Option<String>,
        /// Show most-changed variables (ranked by distinct values).
        #[arg(long, conflicts_with = "variable")]
        most_changed: bool,
        /// Show memory usage (current and peak).
        #[arg(long, conflicts_with_all = ["variable", "most_changed"])]
        memory: bool,
        /// Limit for --most-changed (default 10).
        #[arg(long, value_name = "N", default_value_t = 10)]
        limit: usize,
    },
    /// Delete an execution (workflow record + agent loop if present).
    Delete {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Skip confirmation.
        #[arg(long, alias = "yes")]
        force: bool,
    },
    /// Cleanup completed agent loop executions.
    Cleanup {
        /// Only cleanup before this ISO8601 timestamp or epoch millis (stored as string, lexicographic compare not used; any value triggers cleanup).
        #[arg(long, value_name = "TIMESTAMP")]
        before: Option<String>,
    },
}
