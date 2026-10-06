//! Entity subcommand arguments (skills, variables, messages, tasks).

use clap::Subcommand;

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
