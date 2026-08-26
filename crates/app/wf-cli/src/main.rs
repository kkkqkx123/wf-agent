//! wf binary entry point: parse arguments and hand off to the library.

use clap::Parser;

use wf_cli::args::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = wf_cli::run(cli).await {
        // Diagnostics go to stderr; stdout stays reserved for business output.
        eprintln!("wf: {err}");
        std::process::exit(i32::from(err.exit_code()));
    }
}
