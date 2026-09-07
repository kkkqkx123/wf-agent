//! Interactive form resolution.
//!
//! Resolution order:
//!   --tui  >  subcommand (headless run)  >  --no-tui
//!   >  stdout not a TTY (headless run)  >  TTY default (full TUI).

use std::io::IsTerminal;

use crate::args::{Cli, Command};
use crate::error::{CliError, CliResult};

/// Interactive / non-interactive CLI form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliMode {
    /// Single headless agent session (`wf run` or piped stdin).
    Run,
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
    /// Session id to replay for interactive forms (resolved from
    /// `--session`).
    pub resume_session: Option<String>,
    /// Whether `--resume` was requested (resolve latest session at
    /// startup).
    pub resume_latest: bool,
}

/// Mode resolver. TTY flags are injected so unit tests can exercise every
/// branch without a real terminal.
pub struct ModeResolver;

impl ModeResolver {
    /// Resolve the interactive form for the given arguments.
    ///
    /// `is_stdin_tty` / `is_stdout_tty` are `IsTerminal` answers of the real
    /// streams (injected for testability).
    pub fn resolve(cli: &Cli, is_stdin_tty: bool, is_stdout_tty: bool) -> CliResult<ResolvedMode> {
        cli.validate().map_err(CliError::Arguments)?;

        // 1. Explicit interactive form (highest priority).
        if cli.tui {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Tui,
                stdin_prompt: None,
                resume_session: cli.session.clone(),
                resume_latest: cli.resume,
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
                        resume_session: None,
                        resume_latest: false,
                    });
                }
                Command::DebugMode | Command::DebugTerminal { .. } => {
                    return Ok(ResolvedMode {
                        cli_mode: CliMode::Run,
                        stdin_prompt: None,
                        resume_session: None,
                        resume_latest: false,
                    });
                }
                _ => {
                    return Ok(ResolvedMode {
                        cli_mode: CliMode::Run,
                        stdin_prompt: None,
                        resume_session: None,
                        resume_latest: false,
                    });
                }
            }
        }

        // 3. Explicit headless override.
        if cli.no_tui {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Run,
                stdin_prompt: Self::read_stdin_prompt(cli, is_stdin_tty)?,
                resume_session: None,
                resume_latest: false,
            });
        }

        // 4. Non-TTY stdout falls back to headless (pipe / script).
        if !is_stdout_tty {
            return Ok(ResolvedMode {
                cli_mode: CliMode::Run,
                stdin_prompt: Self::read_stdin_prompt(cli, is_stdin_tty)?,
                resume_session: None,
                resume_latest: false,
            });
        }

        // 5. Interactive TTY default: the full TUI.
        Ok(ResolvedMode {
            cli_mode: CliMode::Tui,
            stdin_prompt: None,
            resume_session: cli.session.clone(),
            resume_latest: cli.resume,
        })
    }

    /// Read the full stdin content as the prompt when stdin is not a TTY and
    /// no positional prompt was given. Empty stdin yields no prompt (the run
    /// layer decides how to handle a missing prompt).
    fn read_stdin_prompt(cli: &Cli, is_stdin_tty: bool) -> CliResult<Option<String>> {
        let already_has_prompt = matches!(
            cli.command,
            Some(Command::Run {
                prompt: Some(_),
                ..
            })
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
    (
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("wf").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn explicit_tui_wins_over_default() {
        let r = ModeResolver::resolve(&cli(&["--tui"]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Tui);
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
    fn tty_default_is_tui() {
        let r = ModeResolver::resolve(&cli(&[]), true, true).unwrap();
        assert_eq!(r.cli_mode, CliMode::Tui);
    }
}
