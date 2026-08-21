//! Interactive form resolution.
//!
//! Resolution order:
//!   --tui  >  --mini  >  subcommand (headless run)  >  --no-tui
//!   >  stdout not a TTY (headless run)  >  TTY default (configurable).

use std::io::IsTerminal;

use crate::args::{Cli, Command};
use crate::error::{CliError, CliResult};

/// Interactive / non-interactive CLI form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliMode {
    /// Single headless agent session (`wf run` or piped stdin).
    Run,
    /// Lightweight inline split-footer session (`--mini`).
    Mini,
    /// Full-screen alt-screen TUI (`--tui`).
    Tui,
}

/// Mode plus any stdin-provided prompt for headless runs.
#[derive(Debug, Clone)]
pub struct ResolvedMode {
    pub cli_mode: CliMode,
    /// Prompt read from stdin when stdin is not a TTY and no positional
    /// prompt was given.
    pub stdin_prompt: Option<String>,
}

/// Mode resolver. TTY flags are injected so unit tests can exercise every
/// branch without a real terminal.
pub struct ModeResolver;

impl ModeResolver {
    /// Resolve the interactive form for the given arguments.
    ///
    /// `is_stdin_tty` / `is_stdout_tty` are `IsTerminal` answers of the real
    /// streams (injected for testability).
    pub fn resolve(
        cli: &Cli,
        is_stdin_tty: bool,
        is_stdout_tty: bool,
    ) -> CliResult<ResolvedMode> {
        cli.validate().map_err(CliError::Arguments)?;

        // 1. Explicit interactive forms (highest priority).
        if cli.tui {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Tui,
                stdin_prompt: None,
            });
        }
        if cli.mini {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Mini,
                stdin_prompt: None,
            });
        }

        // 2. Subcommands: the run subcommand is the headless session form;
        //    other management commands are headless too.
        if let Some(command) = &cli.command {
            match command {
                Command::Run { prompt, .. } => {
                    return Ok(ResolvedMode {
                        cli_mode: CliMode::Run,
                        // Positional prompt wins; otherwise a piped stdin is
                        // read in full as the prompt (echo "p" | wf run).
                        stdin_prompt: match prompt.clone() {
                            Some(p) => Some(p),
                            None => Self::read_stdin_prompt(cli, is_stdin_tty)?,
                        },
                    });
                }
                Command::DebugMode | Command::DebugTerminal { .. } => {
                    return Ok(ResolvedMode {
                        cli_mode: CliMode::Run,
                        stdin_prompt: None,
                    });
                }
            }
        }

        // 3. Explicit headless override.
        if cli.no_tui {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Run,
                stdin_prompt: Self::read_stdin_prompt(cli, is_stdin_tty)?,
            });
        }

        // 4. Non-TTY stdout falls back to headless (pipe / script).
        if !is_stdout_tty {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Run,
                stdin_prompt: Self::read_stdin_prompt(cli, is_stdin_tty)?,
            });
        }

        // 5. Interactive TTY default (configurable via WF_CLI_MODE).
        let default_mode = std::env::var("WF_CLI_MODE").unwrap_or_else(|_| "mini".to_string());
        let cli_mode = match default_mode.as_str() {
            "tui" => CliMode::Tui,
            "mini" => CliMode::Mini,
            other => {
                return Err(CliError::Arguments(format!(
                    "WF_CLI_MODE must be 'mini' or 'tui', got '{other}'"
                )))
            }
        };
        Ok(ResolvedMode {
            cli_mode,
            stdin_prompt: None,
        })
    }

    /// Read the full stdin content as the prompt when stdin is not a TTY and
    /// no positional prompt was given. Empty stdin yields no prompt (the run
    /// layer decides how to handle a missing prompt).
    fn read_stdin_prompt(cli: &Cli, is_stdin_tty: bool) -> CliResult<Option<String>> {
        let already_has_prompt = matches!(
            cli.command,
            Some(Command::Run { prompt: Some(_), .. })
        );
        if already_has_prompt || is_stdin_tty {
            return Ok(None);
        }
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).map_err(|err| {
            CliError::Io(std::io::Error::new(
                err.kind(),
                format!("failed to read prompt from stdin: {err}"),
            ))
        })?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }
}

/// TTY status of the real process streams.
pub fn real_tty_status() -> (bool, bool) {
    (std::io::stdin().is_terminal(), std::io::stdout().is_terminal())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Serializes tests that read/write the `WF_CLI_MODE` env var (parallel
    /// test runners otherwise observe each other's mutations).
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("wf").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn explicit_tui_wins_over_default() {
        let r = ModeResolver::resolve(&cli(&["--tui"]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Tui);
    }

    #[test]
    fn explicit_mini_wins_over_default() {
        let r = ModeResolver::resolve(&cli(&["--mini"]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Mini);
    }

    #[test]
    fn run_subcommand_is_headless() {
        let r = ModeResolver::resolve(&cli(&["run", "hi"]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Run);
        assert_eq!(r.stdin_prompt.as_deref(), Some("hi"));
    }

    #[test]
    fn run_subcommand_without_prompt_yields_none_on_tty_stdin() {
        // Piped stdin (non-TTY) reading is exercised end-to-end; here we
        // verify a TTY stdin keeps the prompt absent instead of blocking.
        let r = ModeResolver::resolve(&cli(&["run"]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Run);
        assert!(r.stdin_prompt.is_none());
    }

    #[test]
    fn non_tty_stdout_falls_back_to_headless() {
        let r = ModeResolver::resolve(&cli(&[]), true, false).unwrap();
        assert_eq!(r.cli_mode, CliMode::Run);
    }

    #[test]
    fn no_tui_forces_headless_even_on_tty() {
        let r = ModeResolver::resolve(&cli(&["--no-tui"]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Run);
    }

    #[test]
    fn tty_default_is_mini() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::remove_var("WF_CLI_MODE");
        let r = ModeResolver::resolve(&cli(&[]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Mini);
    }

    #[test]
    fn env_var_overrides_default() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("WF_CLI_MODE", "tui");
        let r = ModeResolver::resolve(&cli(&[]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Tui);
        std::env::remove_var("WF_CLI_MODE");
    }

    #[test]
    fn invalid_env_value_is_rejected() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("WF_CLI_MODE", "bogus");
        let err = ModeResolver::resolve(&cli(&[]), true, true).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        std::env::remove_var("WF_CLI_MODE");
    }

    #[test]
    fn tui_mini_conflict_is_rejected() {
        let err = ModeResolver::resolve(&cli(&["--tui", "--mini"]), true, true).unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}
