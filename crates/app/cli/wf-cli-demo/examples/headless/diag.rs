//! Headless diagnostics demo: the stderr channel for one run.
//!
//! Shows tool lifecycle lines and usage notes as captured on the
//! diagnostics channel.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example headless_diag
//! cargo run -p wf-cli-demo --example headless_diag -- --emit
//! ```

use wf_cli_demo::emit;
use wf_cli_shared::run_diag::DiagWriter;

fn section() -> String {
    let mut diag = DiagWriter::buffer();
    diag.line("▲ read_file").expect("diag line");
    diag.line("✓ read_file").expect("diag line");
    diag.line("✗ write_file: permission denied")
        .expect("diag line");
    diag.line("usage: 10 prompt + 20 completion tokens")
        .expect("diag line");
    let mut out = String::new();
    out.push_str("stderr channel (uncolored buffer):\n");
    out.push_str(&diag.snapshot());
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("headless", "diag", &section());
        return;
    }
    println!("headless diag: stderr carries lifecycle, stdout stays clean");
    print!("{}", section());
}
