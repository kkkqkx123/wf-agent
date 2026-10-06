//! Tooling subcommand arguments (tools, scripts, templates, triggers).

use clap::Subcommand;

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

/// Trigger subcommands: template registry plus the firing ledger.
///
/// Mirrors the `wf-api::trigger` domain (`template` + `execution`) and the
/// `wf-server` trigger routes; `History`/`Executions` read the event-driven
/// listener ledger, `List`/`Show`/`Save`/`Delete` manage templates.
#[derive(Debug, Clone, Subcommand)]
pub enum TriggerSub {
    /// List trigger templates.
    List {
        /// Filter by trigger type (schedule, event, condition).
        #[arg(long = "type", value_name = "TYPE")]
        trigger_type: Option<String>,
        /// Filter by category.
        #[arg(long, value_name = "CATEGORY")]
        category: Option<String>,
        /// Filter by tags (comma-separated).
        #[arg(long, value_name = "TAGS")]
        tags: Option<String>,
        /// Filter by enabled flag.
        #[arg(long)]
        enabled: Option<bool>,
        /// Filter by name substring.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    /// Show a single trigger template.
    Show {
        /// Template id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Save a trigger template from a JSON file.
    Save {
        /// Path to trigger template JSON file.
        #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::FilePath)]
        file: String,
    },
    /// Delete a trigger template.
    Delete {
        /// Template id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Execution history of triggers for one execution (newest first).
    History {
        /// Execution id.
        #[arg(long, value_name = "ID")]
        execution: String,
        /// Filter by trigger name.
        #[arg(long, value_name = "NAME")]
        trigger: Option<String>,
    },
    /// List trigger firing records (ledger).
    Executions {
        /// Filter by trigger name.
        #[arg(long, value_name = "NAME")]
        trigger: Option<String>,
        /// Filter by execution id.
        #[arg(long, value_name = "ID")]
        execution: Option<String>,
        /// Filter by workflow id.
        #[arg(long, value_name = "ID")]
        workflow: Option<String>,
        /// Filter by outcome (completed | failed | abandoned).
        #[arg(long, value_name = "OUTCOME")]
        outcome: Option<String>,
        /// Maximum number of results.
        #[arg(long, value_name = "N")]
        limit: Option<u64>,
        /// Offset into the result set.
        #[arg(long, value_name = "N")]
        offset: Option<u64>,
    },
    /// Show a single trigger firing record.
    ExecutionShow {
        /// Firing record id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Firing statistics by trigger name.
    Stats,
    /// Delete firing records older than a timestamp (epoch millis, default now).
    Cleanup {
        /// Only delete records triggered before this epoch millis.
        #[arg(long, value_name = "MILLIS")]
        older_than: Option<i64>,
    },
}
