//! Mini markdown demo: one sample through every stdout presentation.
//!
//! Shows what the user sees on stdout for `raw`, `plain`, and `styled`
//! (with and without color), plus escape stripping of model output.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example mini_markdown
//! cargo run -p wf-cli-demo --example mini_markdown -- --emit
//! ```

use wf_cli_demo::{emit, mini_render};
use wf_cli_shared::display::MarkdownMode;

fn section() -> String {
    let mut out = String::new();
    out.push_str("raw verbatim:\n");
    out.push_str(&mini_render::strip_terminal_escapes(
        mini_render::SAMPLE_MARKDOWN,
    ));
    out.push_str("\nplain text:\n");
    out.push_str(&mini_render::render_plain(mini_render::SAMPLE_MARKDOWN));
    let styled = mini_render::render_styled("**bold** and `code`");
    out.push_str("\nstyled bytes contain SGR: ");
    out.push_str(&format!("{}\n", styled.contains("\x1b[")));
    out.push_str(&format!(
        "stripped styled equals plain: {}\n",
        mini_render::strip_terminal_escapes(&styled)
            == mini_render::render_plain("**bold** and `code`")
    ));
    out.push_str(&format!(
        "styled without color degrades to plain (no SGR in plain): {}\n",
        !mini_render::render_plain("**hi**\n").contains('\x1b')
    ));
    out.push_str(&format!(
        "model escapes stripped: {:?}\n",
        mini_render::strip_terminal_escapes("\x1b[31mred\x1b[0m ok")
    ));
    out.push_str(&format!(
        "markdown mode parses: {:?}\n",
        MarkdownMode::parse_opt("plain")
    ));
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("mini", "markdown_modes", &section());
        return;
    }
    println!("mini markdown: stdout presentation per --markdown mode");
    print!("{}", section());
}
