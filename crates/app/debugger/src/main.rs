use clap::Parser;
use wf_debugger::cli::{run, DebuggerCli};

fn main() -> anyhow::Result<()> {
    let cli = DebuggerCli::parse();
    let code = run(cli)?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}
