//! Headless stream demo: delta buffering before the sink.
//!
//! Shows which delta bytes are ready after each push, what a boundary
//! flush drains, and how text sinks stream chunks while structured sinks
//! drop them.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example headless_stream
//! cargo run -p wf-cli-demo --example headless_stream -- --emit
//! ```

use wf_cli_demo::{emit, headless_common};
use wf_cli_shared::output::OutputFormat;
use wf_cli_shared::run_render::DeltaBuffer;

fn section() -> String {
    let mut buffer = DeltaBuffer::new(8 * 1024);
    let mut out = String::new();
    out.push_str(&format!("push `hel`: {:?}\n", buffer.push("hel")));
    out.push_str(&format!(
        "push `lo wo<nl>ta`: {:?}\n",
        buffer.push("lo wo\nta")
    ));
    out.push_str(&format!("push `il<nl>`: {:?}\n", buffer.push("il\n")));
    out.push_str(&format!("remaining: {:?}\n", buffer.take_remaining()));
    out.push_str(&format!(
        "text sink streams chunks: {:?}\n",
        headless_common::text_via_chunks(OutputFormat::Text, &["hello\nbig ", "world"])
    ));
    out.push_str(&format!(
        "json sink drops chunks: {:?}\n",
        headless_common::text_via_chunks(OutputFormat::Json, &["hello\n"])
    ));
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("headless", "delta", &section());
        return;
    }
    println!("headless stream: complete lines flush, tails wait");
    print!("{}", section());
}
