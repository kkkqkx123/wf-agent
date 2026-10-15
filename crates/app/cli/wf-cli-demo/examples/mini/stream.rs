//! Mini stream demo: complete-lines-only buffering over chunked deltas.
//!
//! Shows which bytes are ready after each delta and what `flush` drains at
//! a boundary, including the reference-link hold-back.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example mini_stream
//! cargo run -p wf-cli-demo --example mini_stream -- --emit
//! ```

use wf_cli_demo::{emit, mini_render::StreamRenderer};
use wf_cli_shared::display::MarkdownMode;

fn section() -> String {
    let mut plain = StreamRenderer::new(MarkdownMode::Plain, false);
    let mut out = String::new();
    out.push_str(&format!(
        "chunk `**hello** ` held back: {:?}\n",
        plain.push("**hello** ")
    ));
    out.push_str(&format!(
        "chunk `world\\n` settles a line: {:?}\n",
        plain.push("world\n")
    ));
    out.push_str(&format!("chunk `tail` held back: {:?}\n", plain.push("tail")));
    out.push_str(&format!("flush drains tail: {:?}\n", plain.flush()));
    let mut reference = StreamRenderer::new(MarkdownMode::Plain, false);
    out.push_str(&format!(
        "reference link held back: {:?}\n",
        reference.push("see [a][b] now\n")
    ));
    out.push_str(&format!("reference flush: {:?}\n", reference.flush()));
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("mini", "stream_lines", &section());
        return;
    }
    println!("mini stream: deltas settle whole lines only");
    print!("{}", section());
}
