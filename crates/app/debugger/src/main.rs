use clap::error::ErrorKind;
use clap::Parser;

use wf_common::exit_code as exit;
use wf_debugger::cli::{run, DebuggerCli};

fn main() {
    let cli = match DebuggerCli::try_parse() {
        Ok(cli) => cli,
        Err(err) => match err.kind() {
            // Help and version are successful outcomes.
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => err.exit(),
            // Usage errors use the shared sysexits code, never clap's 2.
            _ => {
                eprintln!("wf-debug: {err}");
                std::process::exit(i32::from(exit::USAGE));
            }
        },
    };
    match run(cli) {
        Ok(code) => {
            if code != exit::OK as i32 {
                std::process::exit(code);
            }
        }
        Err(err) => {
            // Trace/snapshot load and parse failures are input problems:
            // a missing or unreadable file is EX_NOINPUT.
            eprintln!("wf-debug: {err:#}");
            std::process::exit(i32::from(exit::NOINPUT));
        }
    }
}
