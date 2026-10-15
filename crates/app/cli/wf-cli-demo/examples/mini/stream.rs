//! Mini stream demo: settled-region buffering over chunked deltas.
//!
//! Shows which bytes are ready after each delta and what `flush` drains at
//! a boundary. An in-flight block (the current paragraph, an open fence,
//! an unclosed table) stays held until no later delta can rewrite it —
//! bytes already written to stdout can never be retracted — so a
//! multi-line emphasis or a setext underline can no longer emit a stale
//! line. The demonstration includes the reference-link hold-back and the
//! table hold-back (a table that forms mid-stream would rewrite
//! already-emitted bytes, so it drains whole at a boundary).
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
        "chunk `world\\n` still inside the paragraph: {:?}\n",
        plain.push("world\n")
    ));
    out.push_str(&format!(
        "chunk `tail` extends the same paragraph: {:?}\n",
        plain.push("tail")
    ));
    out.push_str(&format!(
        "blank line settles the whole paragraph: {:?}\n",
        plain.push("\n\n")
    ));
    out.push_str(&format!(
        "chunk `more` starts a new block: {:?}\n",
        plain.push("more")
    ));
    out.push_str(&format!("flush drains the tail: {:?}\n", plain.flush()));
    let mut reference = StreamRenderer::new(MarkdownMode::Plain, false);
    out.push_str(&format!(
        "reference link held back: {:?}\n",
        reference.push("see [a][b] now\n")
    ));
    out.push_str(&format!("reference flush: {:?}\n", reference.flush()));
    let mut table = StreamRenderer::new(MarkdownMode::Plain, false);
    out.push_str(&format!(
        "table header held back: {:?}\n",
        table.push("| area | status |\n")
    ));
    out.push_str(&format!(
        "table delimiter held back: {:?}\n",
        table.push("| --- | --- |\n")
    ));
    out.push_str(&format!(
        "table row held back: {:?}\n",
        table.push("| auth | fixed |\n")
    ));
    out.push_str(&format!(
        "table flush drains whole block: {:?}\n",
        table.flush()
    ));
    let mut emphasis = StreamRenderer::new(MarkdownMode::Plain, false);
    out.push_str(&format!(
        "emphasis opening on a line held back: {:?}\n",
        emphasis.push("hello *wor\n")
    ));
    out.push_str(&format!(
        "emphasis closing on the next line: {:?}\n",
        emphasis.push("ld* end\n")
    ));
    out.push_str(&format!(
        "paragraph settles the emphasis whole: {:?}\n",
        emphasis.push("\n\n")
    ));
    let mut heading = StreamRenderer::new(MarkdownMode::Plain, false);
    out.push_str(&format!(
        "setext candidate held back: {:?}\n",
        heading.push("Title\n")
    ));
    out.push_str(&format!(
        "underline extends the same block: {:?}\n",
        heading.push("=====\n")
    ));
    out.push_str(&format!(
        "heading settles at the blank line: {:?}\n",
        heading.push("\n")
    ));
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("mini", "stream_lines", &section());
        return;
    }
    println!("mini stream: deltas settle whole blocks only");
    print!("{}", section());
}
