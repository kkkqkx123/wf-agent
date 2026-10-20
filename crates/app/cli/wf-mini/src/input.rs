//! Line-discipline input: dual pipelines, slash commands, history.
//!
//! The process reads stdin through exactly one source, chosen once at
//! startup by stdin TTY state and never switched mid-run:
//! - pipe pipeline (stdin not a TTY): one pump thread forwards raw byte
//!   chunks into a channel; [`LineReader`] assembles cooked lines.
//! - TTY pipeline (stdin is a TTY): the crossterm event stream is the only
//!   stdin reader; raw prompts consume key events, cooked prompts and
//!   approvals assemble lines from key events up to Enter.
//!
//! Two consumers take turns on the active source, never concurrently:
//! cooked [`LineReader`] use (approvals, piped stdin, TTY cooked fallback)
//! and the raw-mode prompt editor ([`crate::editor`]) which borrows the
//! stream or receiver for one prompt and hands it back afterwards.

mod commands;
mod history;
mod reader;

pub use commands::{parse_command, SlashCommand};
pub use history::{
    load_file_config as load_history_file_config, resolve_history_limit, resolve_history_path,
    History,
};
pub use reader::LineReader;
