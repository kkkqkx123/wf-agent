//! Mini session demo: what enters memory after a turn.
//!
//! Only completed turns enter the transcript; interrupted and failed turns
//! never do. Restore keeps user and assistant text only.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example mini_session
//! cargo run -p wf-cli-demo --example mini_session -- --emit
//! ```

use wf_cli_demo::{emit, mini_memory};

fn section() -> String {
    format!("{}{}", mini_memory::push_summary(), mini_memory::restore_summary())
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("mini", "transcript", &section());
        return;
    }
    println!("mini session: completed turns become the next turn's context");
    print!("{}", section());
}
