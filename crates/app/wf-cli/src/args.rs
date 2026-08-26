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
    },
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
    },
}

/// Execution subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ExecutionSub {
    /// List agent loop executions.
    List {
        /// Filter by status (running, paused, completed, failed).
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
    },
    /// Show a single execution summary.
    Show {
        /// Execution id.
        #[arg(value_name = "ID")]
        id: String,
    },
}

/// LLM profile subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum LlmProfileSub {
    /// List registered LLM profiles.
    List,
}

/// Skill subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum SkillSub {
    /// List registered skills.
    List,
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
