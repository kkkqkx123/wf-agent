//! Command line argument parsing (clap derive).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::output::OutputFormat;

/// wf-agent command line interface: headless run, mini and full TUI modes.
///
/// Interactive forms (mini / full TUI) are entered with no subcommand; the
/// `run` subcommand executes a single headless agent session.
#[derive(Debug, Clone, Parser)]
#[command(name = "wf", version, about, propagate_version = true)]
pub struct Cli {
    /// Enter the full-screen TUI (alt-screen; requires a TTY).
    #[arg(long)]
    pub tui: bool,
    /// Enter the lightweight mini session (inline split-footer; requires a
    /// TTY).
    #[arg(long)]
    pub mini: bool,
    /// Force headless mode even when stdout is a TTY (no interactive UI).
    #[arg(long)]
    pub no_tui: bool,
    /// Output format for command and run output.
    #[arg(long, short = 'o', value_enum, global = true, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,
    /// Also tee command output into this file (any mode).
    #[arg(long, global = true)]
    pub log: Option<PathBuf>,
    /// Disable ANSI colors in text output.
    #[arg(long, global = true)]
    pub no_color: bool,
    /// Agent definition id for interactive sessions (defaults to the primary
    /// agent). Headless runs use `wf run --agent` instead.
    #[arg(long)]
    pub agent: Option<String>,
    /// LLM profile id for interactive sessions (defaults to `default`).
    /// Headless runs use `wf run --model` instead.
    #[arg(long)]
    pub model: Option<String>,
    /// Initial prompt for the mini session.
    #[arg(long, short = 'p')]
    pub prompt: Option<String>,
    /// Session id to resume in an interactive form.
    #[arg(long)]
    pub session: Option<String>,
    /// Resume the most recent session in an interactive form.
    #[arg(long)]
    pub resume: bool,
    /// Storage backend spec: `memory` or `sqlite:<path>`.
    #[arg(long, global = true)]
    pub storage: Option<String>,
    /// Log level: `trace`, `debug`, `info`, `warn`, `error`.
    #[arg(long = "log-level", global = true)]
    pub log_level: Option<String>,
    /// Project root for file-layer config (`configs/infrastructure`).
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Execution timeout in milliseconds.
    #[arg(long, global = true)]
    pub timeout: Option<u64>,
    /// Tool approval mode: `auto`, `llm`, `manual`.
    #[arg(long, global = true)]
    pub approval: Option<String>,
    /// Subcommand; absent selects an interactive form.
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Validate cross-option compatibility; returns an error message on
    /// invalid combinations.
    pub fn validate(&self) -> Result<(), String> {
        if self.tui && self.mini {
            return Err("--tui and --mini are mutually exclusive".to_string());
        }
        if self.command.is_some() && (self.tui || self.mini) {
            return Err(
                "interactive flags (--tui/--mini) cannot be combined with a subcommand".to_string(),
            );
        }
        if self.no_tui && (self.tui || self.mini) {
            return Err("--no-tui conflicts with --tui/--mini".to_string());
        }
        if self.prompt.is_some() && !self.mini {
            return Err("--prompt/-p requires --mini".to_string());
        }
        if self.session.is_some() && self.resume {
            return Err("--session and --resume are mutually exclusive".to_string());
        }
        // Interactive-only options must not leak into subcommands (the run
        // subcommand has its own --agent/--model).
        if self.command.is_some() {
            if self.session.is_some() || self.resume {
                return Err(
                    "--session/--resume require an interactive form (no subcommand)".to_string(),
                );
            }
            if self.agent.is_some() || self.model.is_some() {
                return Err(
                    "--agent/--model apply to interactive forms; use `wf run --agent/--model`"
                        .to_string(),
                );
            }
        }
        // --no-tui forces headless even on a TTY; the interactive options
        // would be silently ignored, so reject the combination up front.
        if self.no_tui && (self.session.is_some() || self.resume || self.prompt.is_some()) {
            return Err(
                "--session/--resume/--prompt require an interactive form (--no-tui forces headless)"
                    .to_string(),
            );
        }
        if let Some(storage) = &self.storage {
            Self::validate_storage(storage)?;
        }
        if let Some(level) = &self.log_level {
            Self::validate_log_level(level)?;
        }
        if let Some(approval) = &self.approval {
            Self::validate_approval(approval)?;
        }
        if let Some(timeout) = self.timeout {
            if timeout == 0 {
                return Err("--timeout must be greater than 0".to_string());
            }
        }
        if (self.session.is_some() || self.resume) && Self::storage_is_memory(&self.storage) {
            return Err(
                "--session/--resume requires --storage sqlite:<path> (memory storage cannot persist sessions)"
                    .to_string(),
            );
        }
        if let Some(Command::Run {
            workflow,
            input,
            prompt,
            ..
        }) = &self.command
        {
            if input.is_some() && workflow.is_none() {
                return Err("--input requires --workflow".to_string());
            }
            if workflow.is_some() && prompt.is_some() {
                return Err(
                    "positional prompt cannot be combined with --workflow; use --input for workflow input"
                        .to_string(),
                );
            }
            if let Some(input_str) = input {
                if serde_json::from_str::<serde_json::Value>(input_str).is_err() {
                    return Err(format!("invalid --input JSON: {input_str}"));
                }
            }
        }
        Ok(())
    }

    fn storage_is_memory(storage: &Option<String>) -> bool {
        match storage.as_deref() {
            None => true,
            Some("memory") => true,
            Some(s) if s.starts_with("sqlite:") => false,
            Some("sqlite") => false,
            _ => true,
        }
    }

    fn validate_storage(spec: &str) -> Result<(), String> {
        if spec == "memory" || spec == "sqlite" || spec.starts_with("sqlite:") {
            Ok(())
        } else {
            Err(format!(
                "invalid --storage '{spec}': expected 'memory' or 'sqlite:<path>'"
            ))
        }
    }

    fn validate_log_level(level: &str) -> Result<(), String> {
        let lower = level.to_ascii_lowercase();
        match lower.as_str() {
            "trace" | "debug" | "info" | "warn" | "warning" | "error" => Ok(()),
            _ => Err(format!(
                "invalid --log-level '{level}': expected trace|debug|info|warn|error"
            )),
        }
    }

    fn validate_approval(mode: &str) -> Result<(), String> {
        let lower = mode.to_ascii_lowercase();
        match lower.as_str() {
            "auto" | "llm" | "manual" => Ok(()),
            _ => Err(format!(
                "invalid --approval '{mode}': expected auto|llm|manual"
            )),
        }
    }
}

/// Subcommands (headless, non-interactive management surface).
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Run a single headless agent session and exit.
    ///
    /// The prompt is taken from the positional argument; when absent and
    /// stdin is not a TTY, the full stdin content is used as the prompt.
    Run {
        /// Prompt to execute.
        #[arg(value_name = "PROMPT")]
        prompt: Option<String>,
        /// Agent definition id to run (defaults to the primary agent).
        #[arg(long)]
        agent: Option<String>,
        /// LLM profile id to run against (defaults to `default`).
        #[arg(long)]
        model: Option<String>,
        /// Pre-authorize tools or commands whose name starts with this
        /// prefix (repeatable, e.g. --approve-prefix git).
        #[arg(long = "approve-prefix", value_name = "PREFIX")]
        approve_prefixes: Vec<String>,
        /// Workflow id to execute instead of an agent turn.
        #[arg(long)]
        workflow: Option<String>,
        /// Workflow input as JSON (requires --workflow).
        #[arg(long)]
        input: Option<String>,
    },
    /// Print resolved CLI mode / output routing (diagnostics).
    DebugMode,
    /// Terminal facility probe: guard enter/restore,
    /// `with_restored` external command, theme detection (diagnostics).
    DebugTerminal {
        /// Also exercise the alternate screen (full-TUI mode set).
        #[arg(long)]
        alt_screen: bool,
        /// Command to run inside the `with_restored` window (default:
        /// `$EDITOR`, or `true` when unset).
        #[arg(long)]
        exec: Option<String>,
    },
    /// Workflow management commands (read-only subset).
    Workflow {
        #[command(subcommand)]
        sub: WorkflowSub,
    },
    /// Execution management commands (read-only subset).
    Execution {
        #[command(subcommand)]
        sub: ExecutionSub,
    },
    /// LLM profile management commands (read-only subset).
    #[command(name = "llm-profile")]
    LlmProfile {
        #[command(subcommand)]
        sub: LlmProfileSub,
    },
    /// Skill management commands (read-only subset).
    Skill {
        #[command(subcommand)]
        sub: SkillSub,
    },
    /// Unified cross-resource search.
    Search {
        /// Search query string.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Limit total results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Query execution records with filtering and pagination.
    Query {
        /// Filter by status (e.g. completed, failed, running).
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Filter by workflow id.
        #[arg(long, value_name = "ID")]
        workflow_id: Option<String>,
        /// Maximum number of records (default 100).
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Sort field.
        #[arg(long, value_name = "FIELD")]
        sort: Option<String>,
        /// Sort descending.
        #[arg(long)]
        desc: bool,
        /// Offset.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
        /// Aggregation (count, sum:field, avg:field, min:field, max:field, group_by:field).
        #[arg(long, value_name = "OP")]
        aggregate: Option<String>,
        /// Export format (json, csv, xml).
        #[arg(long, value_name = "FORMAT")]
        export: Option<String>,
        /// Advanced filter expression (field operator value, e.g. 'status eq completed').
        #[arg(long, value_name = "EXPR")]
        filter: Option<String>,
    },
    /// Checkpoint management.
    Checkpoint {
        #[command(subcommand)]
        sub: CheckpointSub,
    },
    /// Audit management.
    Audit {
        #[command(subcommand)]
        sub: AuditSub,
    },
    /// Event management.
    Event {
        #[command(subcommand)]
        sub: EventSub,
    },
    /// Variable management.
    Variable {
        #[command(subcommand)]
        sub: VariableSub,
    },
    /// Message management.
    Message {
        #[command(subcommand)]
        sub: MessageSub,
    },
    /// Tool management.
    Tool {
        #[command(subcommand)]
        sub: ToolSub,
    },
    /// Script management.
    Script {
        #[command(subcommand)]
        sub: ScriptSub,
    },
    /// Trigger management.
    Trigger {
        #[command(subcommand)]
        sub: TriggerSub,
    },
    /// Template management.
    Template {
        #[command(subcommand)]
        sub: TemplateSub,
    },
    /// Approval management.
    Approval {
        #[command(subcommand)]
        sub: ApprovalSub,
    },
    /// Task management.
    Task {
        #[command(subcommand)]
        sub: TaskSub,
    },
    /// Metrics management.
    Metrics {
        #[command(subcommand)]
        sub: MetricsSub,
    },
    /// Analysis management.
    Analysis {
        #[command(subcommand)]
        sub: AnalysisSub,
    },
    /// Show storage health.
    Health,
    /// Show full diagnostics.
    Diagnostics,
}

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
        /// Run in background and return execution id immediately.
        #[arg(long)]
        background: bool,
        /// Stream execution events to stdout (default true for text when not background).
        #[arg(long)]
        stream: bool,
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

/// LLM profile subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum LlmProfileSub {
    /// List registered LLM profiles.
    List,
    /// Show a single profile.
    Show {
        /// Profile id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Create a profile from a JSON file.
    Create {
        /// Path to profile JSON file.
        #[arg(long, value_name = "PATH")]
        file: String,
    },
    /// Update a profile from a JSON file.
    Update {
        /// Profile id.
        #[arg(value_name = "ID")]
        id: String,
        /// Path to profile JSON file.
        #[arg(long, value_name = "PATH")]
        file: String,
    },
    /// Delete a profile.
    Delete {
        /// Profile id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Validate a profile file.
    Validate {
        /// Path to profile JSON file.
        #[arg(value_name = "FILE")]
        file: String,
    },
    /// Get or set the default profile.
    Default {
        /// Set default to this profile id.
        #[arg(long, value_name = "ID")]
        set: Option<String>,
    },
    /// Template operations.
    Template {
        #[command(subcommand)]
        sub: LlmTemplateSub,
    },
    /// Export a profile (masked) to stdout or file.
    Export {
        /// Profile id.
        #[arg(value_name = "ID")]
        id: String,
        /// Output file (stdout when omitted).
        #[arg(long = "file", value_name = "FILE")]
        file: Option<String>,
    },
    /// Import a profile from a JSON file.
    Import {
        /// Path to profile JSON file.
        #[arg(value_name = "FILE")]
        file: String,
    },
}

/// LLM template subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum LlmTemplateSub {
    /// List available templates.
    List {
        /// Filter by kind (e.g. openai, anthropic).
        #[arg(long, value_name = "KIND")]
        kind: Option<String>,
        /// Filter by category.
        #[arg(long, value_name = "CATEGORY")]
        category: Option<String>,
        /// Filter by tags (comma-separated).
        #[arg(long, value_name = "TAGS")]
        tags: Option<String>,
        /// Filter by author.
        #[arg(long, value_name = "AUTHOR")]
        author: Option<String>,
    },
}

/// Skill subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum SkillSub {
    /// List registered skills.
    List,
    /// Query skills by filter.
    Query {
        /// Filter query.
        #[arg(long, value_name = "QUERY")]
        filter: Option<String>,
    },
    /// Show a single skill.
    Show {
        /// Skill name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Enable a skill.
    Enable {
        /// Skill name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Disable a skill.
    Disable {
        /// Skill name.
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Scan for skills.
    Scan,
    /// Reload skills.
    Reload,
    /// Clear skill cache.
    ClearCache,
}

/// Checkpoint subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum CheckpointSub {
    /// Create a checkpoint for an execution.
    Create {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Checkpoint name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// List checkpoints of an execution.
    List {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
        /// Offset into results.
        #[arg(long, value_name = "N")]
        offset: Option<usize>,
    },
    /// Show a checkpoint.
    Show {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Restore a checkpoint.
    Restore {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
        /// Also resume the restored execution.
        #[arg(long)]
        resume: bool,
    },
    /// Delete a checkpoint.
    Delete {
        /// Checkpoint id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show the checkpoint chain of an execution (chronological with transitions).
    Chain {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// GC checkpoints of an execution (optionally before a timestamp).
    Gc {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
        /// Only delete checkpoints before this epoch millis (when omitted delete all of the execution).
        #[arg(long, value_name = "TIMESTAMP")]
        before: Option<i64>,
    },
}

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

/// Variable subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum VariableSub {
    /// List variables of an execution.
    List {
        /// Execution id.
        #[arg(long, value_name = "ID")]
        execution: String,
        /// Filter by scope.
        #[arg(long, value_name = "SCOPE")]
        scope: Option<String>,
    },
    /// Get a variable.
    Get {
        /// Execution id.
        #[arg(long, value_name = "ID")]
        execution: String,
        /// Scope.
        #[arg(long, value_name = "SCOPE", default_value = "default")]
        scope: String,
        /// Variable name.
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// Set a variable.
    Set {
        /// Execution id.
        #[arg(long, value_name = "ID")]
        execution: String,
        /// Scope.
        #[arg(long, value_name = "SCOPE", default_value = "default")]
        scope: String,
        /// Variable name.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Value as JSON.
        #[arg(long, value_name = "JSON")]
        value: String,
    },
    /// Delete a variable.
    Delete {
        /// Execution id.
        #[arg(long, value_name = "ID")]
        execution: String,
        /// Scope.
        #[arg(long, value_name = "SCOPE", default_value = "default")]
        scope: String,
        /// Variable name.
        #[arg(long, value_name = "NAME")]
        name: String,
    },
}

/// Message subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum MessageSub {
    /// List messages of an execution.
    List {
        /// Execution id.
        #[arg(long, value_name = "ID")]
        execution: String,
        /// Filter by role.
        #[arg(long, value_name = "ROLE")]
        role: Option<String>,
        /// Limit.
        #[arg(long, value_name = "N")]
        limit: Option<u64>,
    },
    /// Search messages.
    Search {
        /// Keyword.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Limit.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
}

/// Tool subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ToolSub {
    /// List registered tools.
    List,
    /// Show a tool.
    Show {
        /// Tool id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Validate tool parameters.
    Validate {
        /// Tool id.
        #[arg(value_name = "ID")]
        id: String,
        /// Parameters as JSON.
        #[arg(long, value_name = "JSON")]
        params: String,
    },
    /// Execute a tool.
    Execute {
        /// Tool id.
        #[arg(value_name = "ID")]
        id: String,
        /// Parameters as JSON.
        #[arg(long, value_name = "JSON")]
        params: String,
        /// Execution id for attribution.
        #[arg(long, value_name = "ID")]
        execution_id: Option<String>,
    },
    /// Save a tool from a JSON file.
    Save {
        /// Path to tool JSON file.
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
    },
    /// Delete a tool.
    Delete {
        /// Tool id.
        #[arg(value_name = "ID")]
        id: String,
        /// Force deletion even if referenced.
        #[arg(long)]
        force: bool,
    },
    /// Enable a tool.
    Enable {
        /// Tool id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Disable a tool.
    Disable {
        /// Tool id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Search tools by keyword.
    Search {
        /// Keyword.
        #[arg(value_name = "QUERY")]
        query: String,
    },
}

/// Script subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ScriptSub {
    /// List scripts.
    List,
    /// Show a script.
    Show {
        /// Script id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Validate script parameters.
    Validate {
        /// Script name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Code.
        #[arg(long, value_name = "CODE")]
        code: Option<String>,
    },
    /// Execute a script.
    Execute {
        /// Script name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Inline code to run.
        #[arg(long, value_name = "CODE")]
        code: Option<String>,
        /// Template to render.
        #[arg(long, value_name = "TEMPLATE")]
        template: Option<String>,
        /// Template args as JSON.
        #[arg(long, value_name = "JSON")]
        args: Option<String>,
    },
    /// Save a script from a JSON file.
    Save {
        /// Path to script JSON file.
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
    },
    /// Delete a script.
    Delete {
        /// Script id.
        #[arg(value_name = "ID")]
        id: String,
        /// Force deletion even if referenced.
        #[arg(long)]
        force: bool,
    },
    /// Enable a script.
    Enable {
        /// Script id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Disable a script.
    Disable {
        /// Script id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Search scripts by keyword.
    Search {
        /// Keyword.
        #[arg(value_name = "QUERY")]
        query: String,
    },
}

/// Trigger subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum TriggerSub {
    /// List triggers.
    List,
    /// Show a trigger.
    Show {
        /// Trigger id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Enable a trigger.
    Enable {
        /// Trigger id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Disable a trigger.
    Disable {
        /// Trigger id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Register a trigger from a JSON file.
    Register {
        /// Path to trigger JSON file.
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
    },
    /// Save (upsert) a trigger from a JSON file.
    Save {
        /// Path to trigger JSON file.
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
    },
    /// Delete a trigger.
    Delete {
        /// Trigger id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Search triggers by keyword.
    Search {
        /// Keyword.
        #[arg(value_name = "QUERY")]
        query: String,
    },
    /// Show trigger statistics.
    Stats,
}

/// Template subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum TemplateSub {
    /// List templates.
    List {
        /// Kind (workflow, agent, node, trigger).
        #[arg(long, value_name = "KIND")]
        kind: Option<String>,
        /// Category filter.
        #[arg(long, value_name = "CATEGORY")]
        category: Option<String>,
        /// Tags filter (comma-separated).
        #[arg(long, value_name = "TAGS")]
        tags: Option<String>,
        /// Author filter.
        #[arg(long, value_name = "AUTHOR")]
        author: Option<String>,
    },
    /// Show a template.
    Show {
        /// Template id.
        #[arg(value_name = "ID")]
        id: String,
        /// Kind.
        #[arg(long, value_name = "KIND")]
        kind: Option<String>,
    },
    /// Clone a template.
    Clone {
        /// Template id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Register a template from a file.
    Register {
        /// Path to template file.
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
        /// Template kind (workflow, agent).
        #[arg(long, value_name = "KIND", default_value = "workflow", value_parser = ["workflow","agent"])]
        kind: String,
        /// Input format (json, toml, auto).
        #[arg(long, value_name = "FORMAT", default_value = "json", value_parser = ["json","toml","auto"])]
        format: String,
    },
    /// Delete a template.
    Delete {
        /// Template id.
        #[arg(value_name = "ID")]
        id: String,
        /// Template kind (workflow, agent).
        #[arg(long, value_name = "KIND", default_value = "workflow", value_parser = ["workflow","agent"])]
        kind: String,
    },
}

/// Approval subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ApprovalSub {
    /// List pending approvals.
    List,
    /// Approve a pending approval.
    Approve {
        /// Agent instance id.
        #[arg(value_name = "INSTANCE")]
        instance: String,
        /// Feature name (auto-derived when omitted).
        #[arg(long, value_name = "FEATURE")]
        feature: Option<String>,
        /// Specific file paths (comma-separated).
        #[arg(long, value_name = "PATHS")]
        paths: Option<String>,
    },
    /// Reject a pending approval.
    Reject {
        /// Agent instance id.
        #[arg(value_name = "INSTANCE")]
        instance: String,
    },
}

/// Task subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum TaskSub {
    /// List tasks.
    List {
        /// Filter by status.
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Filter by task type.
        #[arg(long, value_name = "TYPE")]
        task_type: Option<String>,
        /// Limit.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Show a task.
    Show {
        /// Task id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show task statistics.
    Stats,
    /// Cancel a task.
    Cancel {
        /// Task id.
        #[arg(value_name = "ID")]
        id: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("wf").chain(args.iter().copied()))
    }

    #[test]
    fn parses_run_subcommand_with_prompt() {
        let cli = parse(&["run", "hello world"]).unwrap();
        let Some(Command::Run {
            prompt,
            agent,
            model,
            approve_prefixes,
            ..
        }) = cli.command
        else {
            panic!("expected run command");
        };
        assert_eq!(prompt.as_deref(), Some("hello world"));
        assert!(agent.is_none());
        assert!(model.is_none());
        assert!(approve_prefixes.is_empty());
    }

    #[test]
    fn parses_run_model_and_repeatable_approve_prefixes() {
        let cli = parse(&[
            "run",
            "hi",
            "--model",
            "mock",
            "--approve-prefix",
            "git",
            "--approve-prefix",
            "cargo ",
        ])
        .unwrap();
        let Some(Command::Run {
            model,
            approve_prefixes,
            ..
        }) = cli.command
        else {
            panic!("expected run command");
        };
        assert_eq!(model.as_deref(), Some("mock"));
        assert_eq!(
            approve_prefixes,
            vec!["git".to_string(), "cargo ".to_string()]
        );
    }

    #[test]
    fn parses_interactive_flags() {
        let cli = parse(&["--mini"]).unwrap();
        assert!(cli.mini);
        assert!(!cli.tui);

        let cli = parse(&["--tui"]).unwrap();
        assert!(cli.tui);
    }

    #[test]
    fn output_format_flag_parses_all_variants() {
        for (flag, expected) in [
            ("text", OutputFormat::Text),
            ("json", OutputFormat::Json),
            ("jsonl", OutputFormat::JsonLines),
            ("silent", OutputFormat::Silent),
        ] {
            let cli = parse(&["run", "-o", flag]).unwrap();
            assert_eq!(cli.output, expected, "flag {flag}");
            let cli = parse(&["run", "--output", flag]).unwrap();
            assert_eq!(cli.output, expected, "long flag {flag}");
        }
    }

    #[test]
    fn global_flags_are_visible_before_subcommand() {
        let cli = parse(&["--log", "out.log", "--no-color", "run", "x"]).unwrap();
        assert_eq!(
            cli.log.as_deref().map(|p| p.to_string_lossy().to_string()),
            Some("out.log".into())
        );
        assert!(cli.no_color);
    }

    #[test]
    fn rejects_tui_mini_conflict() {
        let cli = parse(&["--tui", "--mini"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn rejects_subcommand_with_interactive_flag() {
        let cli = parse(&["--mini", "run", "x"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn parses_mini_session_options() {
        let cli = parse(&["--mini", "-p", "hello", "--agent", "ag", "--model", "m"]).unwrap();
        assert!(cli.mini);
        assert_eq!(cli.prompt.as_deref(), Some("hello"));
        assert_eq!(cli.agent.as_deref(), Some("ag"));
        assert_eq!(cli.model.as_deref(), Some("m"));

        let cli = parse(&["--mini", "--session", "abc"]).unwrap();
        assert_eq!(cli.session.as_deref(), Some("abc"));

        let cli = parse(&["--mini", "--resume"]).unwrap();
        assert!(cli.resume);
    }

    #[test]
    fn rejects_prompt_without_mini() {
        let cli = parse(&["-p", "hello"]).unwrap();
        assert!(cli.validate().is_err());
        let cli = parse(&["--tui", "-p", "hello"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn rejects_session_with_subcommand() {
        // The top-level options appear before the subcommand position.
        let cli = parse(&["--session", "abc", "run", "x"]).unwrap();
        assert!(cli.validate().is_err());
        let cli = parse(&["--resume", "run", "x"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn rejects_session_and_resume_together() {
        let cli = parse(&["--mini", "--session", "abc", "--resume"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn rejects_interactive_options_with_no_tui() {
        let cli = parse(&["--no-tui", "--resume"]).unwrap();
        assert!(cli.validate().is_err());
        let cli = parse(&["--no-tui", "--session", "abc"]).unwrap();
        assert!(cli.validate().is_err());
        let cli = parse(&["--no-tui", "-p", "hi"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn rejects_prompt_leaking_into_run_subcommand() {
        // The mini-only -p short flag must not be accepted by `wf run`.
        assert!(parse(&["run", "x", "-p", "y"]).is_err());
    }

    #[test]
    fn run_subcommand_keeps_its_own_agent_model() {
        let cli = parse(&["run", "x", "--agent", "ag", "--model", "m"]).unwrap();
        assert!(cli.validate().is_ok());
        let Some(Command::Run { agent, model, .. }) = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(agent.as_deref(), Some("ag"));
        assert_eq!(model.as_deref(), Some("m"));
        // Top-level interactive options stay untouched.
        assert!(cli.agent.is_none());
        assert!(cli.model.is_none());
    }

    #[test]
    fn session_requires_sqlite_storage() {
        let cli = parse(&["--mini", "--session", "abc"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(err.contains("requires --storage sqlite"), "{err}");

        let cli = parse(&["--mini", "--resume"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(err.contains("requires --storage sqlite"), "{err}");

        let cli = parse(&[
            "--mini",
            "--session",
            "abc",
            "--storage",
            "sqlite:/tmp/wf.db",
        ])
        .unwrap();
        assert!(cli.validate().is_ok());

        let cli = parse(&["--mini", "--resume", "--storage", "sqlite:/tmp/wf.db"]).unwrap();
        assert!(cli.validate().is_ok());

        let cli = parse(&["--mini", "--session", "abc", "--storage", "memory"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn storage_flag_parses_and_validates() {
        let cli = parse(&["--storage", "memory"]).unwrap();
        assert!(cli.validate().is_ok());
        let cli = parse(&["--storage", "sqlite:/tmp/a.db"]).unwrap();
        assert!(cli.validate().is_ok());
        let cli = parse(&["--storage", "postgres://bad"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn log_level_and_approval_flags_validate() {
        for lvl in ["trace", "debug", "info", "warn", "error", "warning"] {
            let cli = parse(&["--log-level", lvl]).unwrap();
            assert!(cli.validate().is_ok(), "{lvl}");
        }
        let cli = parse(&["--log-level", "verbose"]).unwrap();
        assert!(cli.validate().is_err());

        for mode in ["auto", "llm", "manual", "AUTO"] {
            let cli = parse(&["--approval", mode]).unwrap();
            assert!(cli.validate().is_ok(), "{mode}");
        }
        let cli = parse(&["--approval", "strict"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn timeout_and_config_flags_parse() {
        let cli = parse(&["--timeout", "5000"]).unwrap();
        assert_eq!(cli.timeout, Some(5000));
        assert!(cli.validate().is_ok());
        let cli = parse(&["--timeout", "0"]).unwrap();
        assert!(cli.validate().is_err());

        let cli = parse(&["--config", "/tmp/cfg"]).unwrap();
        assert_eq!(
            cli.config
                .as_deref()
                .map(|p| p.to_string_lossy().to_string()),
            Some("/tmp/cfg".into())
        );
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn workflow_flags_parse_and_validate() {
        let cli = parse(&["run", "--workflow", "wf-1"]).unwrap();
        let Some(Command::Run {
            workflow, input, ..
        }) = &cli.command
        else {
            panic!("expected run");
        };
        assert_eq!(workflow.as_deref(), Some("wf-1"));
        assert!(input.is_none());
        assert!(cli.validate().is_ok());

        let cli = parse(&["run", "--workflow", "wf-1", "--input", r#"{"a":1}"#]).unwrap();
        assert!(cli.validate().is_ok());

        let cli = parse(&["run", "--input", r#"{"a":1}"#]).unwrap();
        assert!(cli.validate().is_err());

        let cli = parse(&["run", "prompt", "--workflow", "wf-1"]).unwrap();
        assert!(cli.validate().is_err());

        let cli = parse(&["run", "--workflow", "wf-1", "--input", "bad-json"]).unwrap();
        assert!(cli.validate().is_err());
    }
}
