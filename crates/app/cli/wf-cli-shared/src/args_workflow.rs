//! Workflow-related subcommand arguments (`WorkflowSub`, `WorkflowVersionSub`).

use clap::Subcommand;

/// Workflow subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum WorkflowSub {
    /// List registered workflows.
    List {
        /// Keyword filter (name/description/tags).
        #[arg(long, value_name = "KW")]
        keyword: Option<String>,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<u64>,
        /// Filter by tags (comma-separated, all must match).
        #[arg(long, value_name = "TAGS")]
        tags: Option<String>,
        /// Filter by category.
        #[arg(long, value_name = "CATEGORY")]
        category: Option<String>,
        /// Filter by author.
        #[arg(long, value_name = "AUTHOR")]
        author: Option<String>,
    },
    /// Show a single workflow definition.
    Show {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show the graph structure of a workflow.
    Graph {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Show aggregate summary instead of nodes+edges.
        #[arg(long)]
        summary: bool,
        /// Detect structural cycles.
        #[arg(long)]
        detect_cycles: bool,
        /// Topological sort.
        #[arg(long)]
        topo: bool,
        /// Reachability analysis from a node.
        #[arg(long, value_name = "NODE")]
        reachability: Option<String>,
        /// Neighbors of a node (predecessors + successors).
        #[arg(long, value_name = "NODE")]
        neighbors: Option<String>,
        /// Filter nodes by type (e.g. LLM, SCRIPT).
        #[arg(long = "type", value_name = "TYPE")]
        node_type: Option<String>,
    },
    /// Create a workflow from a JSON file.
    Create {
        /// Path to workflow definition file (JSON).
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
        /// Input format (json, toml, auto).
        #[arg(long, value_name = "FORMAT", default_value = "json", value_parser = ["json","toml","auto"])]
        format: String,
    },
    /// Update a workflow from a JSON file.
    Update {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Path to workflow definition file (JSON).
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
        /// Input format.
        #[arg(long, value_name = "FORMAT", default_value = "json", value_parser = ["json","toml","auto"])]
        format: String,
    },
    /// Delete a workflow.
    Delete {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Skip confirmation.
        #[arg(long, alias = "yes")]
        force: bool,
    },
    /// Clone a workflow.
    Clone {
        /// Source workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// New workflow id (auto-generated when omitted).
        #[arg(long = "as", value_name = "NEW_ID")]
        as_id: Option<String>,
    },
    /// Validate a workflow definition file.
    Validate {
        /// Path to workflow definition file.
        #[arg(value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        file: String,
        /// Input format (json, toml, auto).
        #[arg(long, value_name = "FORMAT", default_value = "auto", value_parser = ["json","toml","auto"])]
        format: String,
    },
    /// Export a workflow to a file or stdout.
    Export {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Output format (json, toml).
        #[arg(long, value_name = "FORMAT", default_value = "json", value_parser = ["json","toml"])]
        format: String,
        /// Output file (stdout when omitted).
        #[arg(long = "file", value_name = "FILE")]
        file: Option<String>,
    },
    /// Import a workflow from a file.
    Import {
        /// Path to workflow file.
        #[arg(value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        file: String,
        /// Input format (auto, json, toml).
        #[arg(long, value_name = "FORMAT", default_value = "auto", value_parser = ["json","toml","auto"])]
        format: String,
    },
    /// Workflow version management.
    Version {
        #[command(subcommand)]
        sub: WorkflowVersionSub,
    },
    /// Rollback a workflow to a previous version.
    Rollback {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Target version.
        #[arg(value_name = "VERSION")]
        version: String,
    },
    /// Show execution graph of a workflow execution.
    ExecutionGraph {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Analyze execution path (paths, critical path, decision points).
        #[arg(long)]
        analysis: bool,
        /// Slow nodes above percentile threshold (0.0-1.0, default 0.8 shows slowest 20%).
        #[arg(long)]
        slow_nodes: bool,
        /// Percentile for --slow-nodes (default 0.8).
        #[arg(long, value_name = "PERCENTILE", default_value_t = 0.8)]
        percentile: f64,
        /// Efficiency analysis (executed vs optimal path).
        #[arg(long)]
        efficiency: bool,
        /// Path probability analysis.
        #[arg(long = "path-probability")]
        path_probability: bool,
        /// Alternative paths at decision points.
        #[arg(long = "alternative-paths")]
        alternative_paths: bool,
    },
}

/// Workflow version subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum WorkflowVersionSub {
    /// List versions of a workflow.
    List {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show a specific version.
    Show {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Version label.
        #[arg(value_name = "VERSION")]
        version: String,
    },
    /// Bump the workflow version (patch/minor/major).
    Bump {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Bump level.
        #[arg(long, value_name = "LEVEL")]
        level: String,
        /// JSON changes object.
        #[arg(long, value_name = "JSON")]
        changes: Option<String>,
        /// Keep original as a version.
        #[arg(long)]
        keep_original: bool,
    },
    /// Diff two versions of a workflow.
    Diff {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
        /// Source version.
        #[arg(long, value_name = "VERSION")]
        from: String,
        /// Target version.
        #[arg(long, value_name = "VERSION")]
        to: String,
    },
    /// Show changelog (aggregated versions).
    Changelog {
        /// Workflow id.
        #[arg(value_name = "ID")]
        id: String,
    },
}
