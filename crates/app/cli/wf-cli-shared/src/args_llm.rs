//! LLM-related subcommand arguments (profiles, providers, templates).

use clap::Subcommand;

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

/// LLM provider subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum LlmProviderSub {
    /// List registered LLM providers.
    List,
    /// Show a single provider definition.
    Show {
        /// Provider id.
        #[arg(value_name = "ID")]
        id: String,
    },
    /// List models advertised by a provider (model discovery).
    Models {
        /// Provider id.
        #[arg(value_name = "ID")]
        id: String,
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
