//! Command line argument parsing (clap derive).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::display::MarkdownMode;
use crate::output::OutputFormat;

pub use crate::args_checkpoint::{ApprovalSub, CheckpointDomain, CheckpointSub};
pub use crate::args_entity::{MessageSub, SkillSub, TaskSub, VariableSub};
pub use crate::args_execution::ExecutionSub;
pub use crate::args_llm::{LlmAuthSub, LlmProfileSub, LlmProviderSub, LlmProxySub, LlmTemplateSub};
pub use crate::args_observe::{AnalysisSub, AuditSub, EventSub, MetricsSub};
pub use crate::args_remote::RemoteSub;
pub use crate::args_tooling::{ScriptSub, TemplateSub, ToolSub, TriggerSub};
pub use crate::args_workflow::{WorkflowSub, WorkflowVersionSub};

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
    /// Session id to resume in the interactive form.
    #[arg(long)]
    pub session: Option<String>,
    /// Resume the most recent session in an interactive form.
    #[arg(long)]
    pub resume: bool,
    /// Execution timeout in milliseconds (server-side run budget).
    #[arg(long, global = true)]
    pub timeout: Option<u64>,
    /// Mini prompt history file path. Overrides the default state-dir file
    /// and `WF_MINI_HISTORY_FILE`. Mini only; ignored by other forms.
    #[arg(long, global = true)]
    pub history_file: Option<std::path::PathBuf>,
    /// Disable mini prompt history persistence. Mini only.
    #[arg(long, global = true)]
    pub no_history: bool,
    /// Assistant markdown presentation on stdout. Mini only; ignored by
    /// other forms. Overrides `WF_MINI_MARKDOWN` and the config file.
    #[arg(long, value_enum, global = true)]
    pub markdown: Option<MarkdownMode>,
    /// Suppress reasoning deltas on stderr. Mini only.
    #[arg(long = "no-reasoning", global = true)]
    pub no_reasoning: bool,
    /// Suppress usage lines on stderr. Mini only.
    #[arg(long = "no-usage", global = true)]
    pub no_usage: bool,
    /// Suppress tool lifecycle lines on stderr (failures still show).
    /// Mini only.
    #[arg(long = "quiet-tools", global = true)]
    pub quiet_tools: bool,
    /// Remote server URL (e.g. http://localhost:3000). Overrides WF_REMOTE env.
    #[arg(long, global = true)]
    pub remote: Option<String>,
    /// API key for remote server (x-api-key header). Also reads WF_API_KEY env.
    #[arg(long, global = true)]
    pub api_key: Option<String>,
    /// Skip interactive confirmation prompts for destructive operations.
    /// Also implied when stdin is not a TTY (scripted/CI mode).
    #[arg(long, global = true)]
    pub yes: bool,
    /// Structured agent context as a JSON object (agent turns only).
    /// Global so headless, mini and TUI share one flag; interactive forms
    /// seed their session context from it.
    #[arg(long, global = true, value_name = "JSON")]
    pub context: Option<String>,
    /// Read structured agent context from a JSON file holding an object;
    /// merged under `--context` (agent turns only). Global for all forms.
    #[arg(long = "context-file", global = true, value_name = "PATH")]
    pub context_file: Option<std::path::PathBuf>,
    /// Subcommand; absent selects an interactive form.
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Validate cross-option compatibility; returns an error message on
    /// invalid combinations.
    pub fn validate(&self) -> Result<(), String> {
        if self.command.is_some() && self.tui {
            return Err("--tui cannot be combined with a subcommand".to_string());
        }
        if self.no_tui && self.tui {
            return Err("--no-tui conflicts with --tui".to_string());
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
        if self.no_tui && (self.session.is_some() || self.resume) {
            return Err(
                "--session/--resume require an interactive form (--no-tui forces headless)"
                    .to_string(),
            );
        }
        if self.history_file.is_some() && self.no_history {
            return Err("--history-file and --no-history are mutually exclusive".to_string());
        }
        if let Some(timeout) = self.timeout {
            if timeout == 0 {
                return Err("--timeout must be greater than 0".to_string());
            }
        }
        if let Some(Command::Run {
            workflow,
            input,
            input_file,
            vars,
            prompt,
            interactive,
            approval_timeout,
            assume_yes,
            ..
        }) = &self.command
        {
            if (input.is_some() || input_file.is_some() || !vars.is_empty()) && workflow.is_none() {
                return Err("--input/--input-file/--var require --workflow".to_string());
            }
            if (self.context.is_some() || self.context_file.is_some()) && workflow.is_some() {
                return Err("--context/--context-file apply to agent turns, not --workflow".to_string());
            }
            for var in vars {
                crate::turn::parse_workflow_var_arg(var)?;
            }
            if workflow.is_some() && prompt.is_some() {
                return Err(
                    "positional prompt cannot be combined with --workflow; use --input for workflow input"
                        .to_string(),
                );
            }
            if *interactive && *assume_yes {
                return Err("--interactive and --assume-yes are mutually exclusive".to_string());
            }
            if let Some(secs) = approval_timeout {
                if *secs == 0 {
                    return Err("--approval-timeout must be greater than 0".to_string());
                }
                if !interactive {
                    return Err("--approval-timeout requires --interactive".to_string());
                }
            }
            if let Some(input_str) = input {
                crate::turn::parse_workflow_input(Some(input_str))?;
            }
            if let Some(context_str) = self.context.as_deref() {
                crate::turn::parse_agent_context(Some(context_str), None)?;
            }
        }
        Ok(())
    }
}

/// Whether the CLI invocation is a headless subcommand that should bypass
/// any TUI mode entirely.
///
/// Single source of truth for headless detection: every `Command` variant
/// is headless by definition (a subcommand never enters an interactive
/// form). `Command::is_headless` carries the per-variant answer so the
/// free function cannot drift from the enum.
pub fn is_headless_command(cli: &Cli) -> bool {
    cli.command.as_ref().is_some_and(|c| c.is_headless())
}

impl Command {
    /// All current subcommands run headless (no interactive form).
    /// Keep this as the single per-variant source; `is_headless_command`
    /// delegates here so new variants cannot be forgotten in one place.
    pub fn is_headless(&self) -> bool {
        true
    }

    /// Management / diagnostic surface vs the single-session `run` form.
    /// `Run`, `DebugMode` and `DebugTerminal` are session/diagnostic
    /// forms; everything else is a management subcommand dispatched
    /// before mode resolution.
    pub fn is_management_command(&self) -> bool {
        !matches!(
            self,
            Command::Run { .. } | Command::DebugMode | Command::DebugTerminal { .. }
        )
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
        /// Answer tool approvals and follow-up questions from stdin: each
        /// prompt renders a `? ...` line to stderr and consumes one stdin
        /// line. Requires a positional prompt so stdin stays free for
        /// answers; stdout keeps pure business output.
        #[arg(long)]
        interactive: bool,
        /// Wait bound in seconds for one stdin answer line with
        /// `--interactive` (default 120); expiry denies approvals and
        /// cancels follow-up answers.
        #[arg(long = "approval-timeout", value_name = "SECS")]
        approval_timeout: Option<u64>,
        /// Approve every routed tool call without prompting (unattended
        /// runs with no human on the line).
        #[arg(long = "assume-yes", short = 'y')]
        assume_yes: bool,
        /// Workflow id to execute instead of an agent turn.
        #[arg(long)]
        workflow: Option<String>,
        /// Workflow input as JSON (requires --workflow).
        #[arg(long)]
        input: Option<String>,
        /// Workflow input from a JSON file holding an object
        /// (requires --workflow); merged under `--input`.
        #[arg(long = "input-file", value_name = "PATH")]
        input_file: Option<std::path::PathBuf>,
        /// One workflow variable assignment `KEY=VALUE` (requires
        /// --workflow, repeatable); merged over `--input-file` and
        /// `--input` with the highest priority. Values parse as JSON when
        /// possible and fall back to plain strings.
        #[arg(long = "var", value_name = "KEY=VALUE")]
        vars: Vec<String>,
        /// Remote server URL for this run (overrides --remote / WF_REMOTE).
        #[arg(long)]
        remote: Option<String>,
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
    /// LLM provider management commands (connection templates + discovery).
    #[command(name = "llm-provider")]
    LlmProvider {
        #[command(subcommand)]
        sub: LlmProviderSub,
    },
    /// LLM authentication: login, logout and status.
    #[command(name = "llm")]
    Llm {
        #[command(subcommand)]
        sub: LlmAuthSub,
    },
    /// Proxy credential management (passwords stay in the credential store).
    #[command(name = "llm-proxy")]
    LlmProxy {
        #[command(subcommand)]
        sub: LlmProxySub,
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
    /// Template management.
    Template {
        #[command(subcommand)]
        sub: TemplateSub,
    },
    /// Trigger management (templates + execution ledger).
    Trigger {
        #[command(subcommand)]
        sub: TriggerSub,
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
    /// TUI display config backup (`tui.toml` only; keymap and theme files
    /// are out of scope, copy those by hand).
    Config {
        #[command(subcommand)]
        sub: ConfigSub,
    },
    /// Remote host registry and execution bundle pull.
    Remote {
        #[command(subcommand)]
        sub: RemoteSub,
    },
}

/// TUI display config backup operations.
#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSub {
    /// Export the live `tui.toml` to `dest`. A missing source exports
    /// defaults so backup never fails on a fresh machine.
    Export {
        /// Destination file path.
        #[arg(value_name = "DEST")]
        dest: PathBuf,
    },
    /// Import `src` over the live `tui.toml`. A malformed source is
    /// rejected and the live file is left untouched.
    Import {
        /// Source backup file path.
        #[arg(value_name = "SRC")]
        src: PathBuf,
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
    fn rejects_subcommand_with_interactive_flag() {
        let cli = parse(&["--tui", "run", "x"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn parses_tui_session_options() {
        let cli = parse(&["--tui", "--agent", "ag", "--model", "m"]).unwrap();
        assert!(cli.tui);
        assert_eq!(cli.agent.as_deref(), Some("ag"));
        assert_eq!(cli.model.as_deref(), Some("m"));

        let cli = parse(&["--tui", "--session", "abc"]).unwrap();
        assert_eq!(cli.session.as_deref(), Some("abc"));

        let cli = parse(&["--tui", "--resume"]).unwrap();
        assert!(cli.resume);
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
        let cli = parse(&["--tui", "--session", "abc", "--resume"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn history_file_and_no_history_are_exclusive() {
        let cli = parse(&["--history-file", "/tmp/h", "--no-history"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(err.contains("mutually exclusive"), "{err}");

        let cli = parse(&["--history-file", "/tmp/h"]).unwrap();
        assert!(cli.validate().is_ok());

        let cli = parse(&["--no-history"]).unwrap();
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn mini_display_flags_parse() {
        let cli = parse(&[
            "--markdown",
            "plain",
            "--no-reasoning",
            "--no-usage",
            "--quiet-tools",
        ])
        .unwrap();
        assert_eq!(cli.markdown, Some(MarkdownMode::Plain));
        assert!(cli.no_reasoning);
        assert!(cli.no_usage);
        assert!(cli.quiet_tools);
        assert!(cli.validate().is_ok());

        let cli = parse(&["--markdown", "styled"]).unwrap();
        assert_eq!(cli.markdown, Some(MarkdownMode::Styled));

        let cli = parse(&[]).unwrap();
        assert!(cli.markdown.is_none());
        assert!(!cli.no_reasoning);
    }

    #[test]
    fn rejects_interactive_options_with_no_tui() {
        let cli = parse(&["--no-tui", "--resume"]).unwrap();
        assert!(cli.validate().is_err());
        let cli = parse(&["--no-tui", "--session", "abc"]).unwrap();
        assert!(cli.validate().is_err());
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
    fn session_flags_validate_without_local_storage() {
        // Sessions live server-side; no local storage flag is required.
        let cli = parse(&["--tui", "--session", "abc"]).unwrap();
        assert!(cli.validate().is_ok());
        let cli = parse(&["--tui", "--resume"]).unwrap();
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn timeout_flag_parses_and_validates() {
        let cli = parse(&["--timeout", "5000"]).unwrap();
        assert_eq!(cli.timeout, Some(5000));
        assert!(cli.validate().is_ok());
        let cli = parse(&["--timeout", "0"]).unwrap();
        assert!(cli.validate().is_err());
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

        let cli = parse(&["run", "--workflow", "wf-1", "--input", "[1,2]"]).unwrap();
        assert!(cli.validate().is_err());
    }

    #[test]
    fn interactive_run_flags_parse() {
        let cli = parse(&["run", "hi", "--interactive"]).unwrap();
        let Some(Command::Run {
            interactive,
            approval_timeout,
            assume_yes,
            ..
        }) = &cli.command
        else {
            panic!("expected run");
        };
        assert!(*interactive);
        assert_eq!(*approval_timeout, None);
        assert!(!assume_yes);
        assert!(cli.validate().is_ok());

        let cli = parse(&["run", "hi", "--interactive", "--approval-timeout", "30"]).unwrap();
        assert!(cli.validate().is_ok());

        let cli = parse(&["run", "hi", "-y"]).unwrap();
        let Some(Command::Run { assume_yes, .. }) = &cli.command else {
            panic!("expected run");
        };
        assert!(*assume_yes);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn interactive_and_assume_yes_are_exclusive() {
        let cli = parse(&["run", "hi", "--interactive", "--assume-yes"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(err.contains("mutually exclusive"), "{err}");
    }

    #[test]
    fn approval_timeout_needs_interactive_and_positive() {
        let cli = parse(&["run", "hi", "--approval-timeout", "30"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(err.contains("--interactive"), "{err}");

        let cli = parse(&["run", "hi", "--interactive", "--approval-timeout", "0"]).unwrap();
        let err = cli.validate().unwrap_err();
        assert!(err.contains("greater than 0"), "{err}");
    }

    #[test]
    fn config_export_import_parse() {
        let cli = parse(&["config", "export", "/tmp/backup.toml"]).unwrap();
        let Some(Command::Config { sub }) = &cli.command else {
            panic!("expected config");
        };
        assert!(matches!(sub, ConfigSub::Export { .. }));
        assert!(cli.validate().is_ok());

        let cli = parse(&["config", "import", "/tmp/backup.toml"]).unwrap();
        let Some(Command::Config { sub }) = &cli.command else {
            panic!("expected config");
        };
        assert!(matches!(sub, ConfigSub::Import { .. }));
        assert!(cli.validate().is_ok());
    }
}
