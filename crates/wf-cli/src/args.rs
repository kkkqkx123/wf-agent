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
            return Err("interactive flags (--tui/--mini) cannot be combined with a subcommand"
                .to_string());
        }
        if self.no_tui && (self.tui || self.mini) {
            return Err("--no-tui conflicts with --tui/--mini".to_string());
        }
        Ok(())
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
    },
    /// Print resolved CLI mode / output routing (diagnostics).
    DebugMode,
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
        let Some(Command::Run { prompt, agent, model, approve_prefixes }) = cli.command else {
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
        let Some(Command::Run { model, approve_prefixes, .. }) = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(model.as_deref(), Some("mock"));
        assert_eq!(approve_prefixes, vec!["git".to_string(), "cargo ".to_string()]);
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
        assert_eq!(cli.log.as_deref().map(|p| p.to_string_lossy().to_string()), Some("out.log".into()));
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
}
