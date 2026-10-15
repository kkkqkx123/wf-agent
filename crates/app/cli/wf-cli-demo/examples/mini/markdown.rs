//! Mini markdown demo: one sample through every reachable rendering branch.
//!
//! Covers the `render_styled` branches (bold, italic, code, link, image,
//! heading depth, fenced and indented code blocks, two-line quotes, rule,
//! inline HTML, strikethrough, table cells, task-list markers), the
//! `StreamRenderer` mode arms (raw, plain, styled with and without color),
//! the unclosed-table streaming hold-back, escape-stripping variants (CSI,
//! OSC BEL, OSC ST, bare ESC, trailing partial, CR) and `MarkdownMode`
//! parsing. The reference-link hold-back branch is covered by `mini_stream`.
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

    let styled = mini_render::render_styled(mini_render::SAMPLE_MARKDOWN);
    let stripped = mini_render::strip_terminal_escapes(&styled);
    out.push_str("\nstyled text (escapes stripped):\n");
    out.push_str(&stripped);
    out.push_str("\nstyled branch checks:\n");
    out.push_str(&format!(
        "  bold span styled: {}\n",
        styled.contains("\x1b[1m")
    ));
    out.push_str(&format!(
        "  italic span styled: {}\n",
        styled.contains("\x1b[3m")
    ));
    out.push_str(&format!(
        "  inline and fenced code cyan: {}\n",
        styled.contains("38;5;14")
    ));
    out.push_str(&format!(
        "  link text underlined: {}\n",
        styled.contains("\x1b[4mdocs")
    ));
    out.push_str(&format!(
        "  image alt text underlined: {}\n",
        styled.contains("\x1b[4mlogo")
    ));
    out.push_str(&format!(
        "  heading printed bold with marker: {}\n",
        styled.contains("\x1b[1m# ")
    ));
    out.push_str(&format!(
        "  fenced block opens dim with language: {}\n",
        styled.contains("\x1b[2m```rust")
    ));
    out.push_str(&format!(
        "  fenced block closes dim: {}\n",
        styled.contains("\x1b[2m```")
    ));
    out.push_str(&format!(
        "  quote marker dim: {}\n",
        styled.contains("\x1b[2m> ")
    ));
    out.push_str(&format!(
        "  quote softbreak repeats marker: {}\n",
        styled.matches("> ").count() == 2
    ));
    out.push_str(&format!(
        "  rule rendered dim: {}\n",
        styled.contains("\x1b[2m---")
    ));
    out.push_str(&format!(
        "  strikethrough crossed out: {}\n",
        styled.contains("\x1b[9m")
    ));
    out.push_str(&format!(
        "  task markers emitted: {}\n",
        styled.contains("[x] retry with backoff") && styled.contains("[ ] alert on failure")
    ));
    out.push_str(&format!(
        "  table cells on their own lines: {}\n",
        stripped.contains("area\nstatus\nauth\nfixed")
    ));
    out.push_str(&format!(
        "  inline html tags dropped: {}\n",
        !styled.contains("<span>")
    ));
    out.push_str(&format!(
        "  every styled span self-resets: {}\n",
        styled.contains("\x1b[0m")
    ));
    out.push_str(&format!(
        "  stripped styled carries no sgr: {}\n",
        !stripped.contains('\x1b')
    ));

    let indented = mini_render::render_styled("    indented code");
    out.push_str("\nindented code block:\n");
    out.push_str(&format!(
        "  opens dim without language: {}\n",
        indented.contains("\x1b[2m```")
    ));
    out.push_str(&format!(
        "  plain keeps content verbatim: {}\n",
        mini_render::render_plain("    indented code") == "indented code"
    ));

    out.push_str("\nheading depth one through six styled bold:\n");
    let depths = (1..=6)
        .map(|level| mini_render::render_styled(&format!("{} title\n", "#".repeat(level))))
        .all(|rendered| rendered.contains("\x1b[1m"));
    out.push_str(&format!("  all six levels bold: {depths}\n"));

    out.push_str("\nstream renderer mode arms:\n");
    let mut raw = mini_render::StreamRenderer::new(MarkdownMode::Raw, false);
    out.push_str(&format!(
        "  raw streams verbatim: {}\n",
        raw.push("**raw** line\n") == "**raw** line\n"
    ));
    let mut plain = mini_render::StreamRenderer::new(MarkdownMode::Plain, false);
    let settled = plain.push("**hello** ") == String::new()
        && plain.push("world\n") == "hello world\n"
        && plain.flush() == String::new();
    out.push_str(&format!("  plain emits complete lines only: {settled}\n"));
    let mut colored = mini_render::StreamRenderer::new(MarkdownMode::Styled, true);
    let colorful = colored.push("**hi**\n");
    out.push_str(&format!(
        "  styled with color emits sgr: {}\n",
        colorful.contains("\x1b[1m") && colorful.contains("\x1b[0m")
    ));
    let mut degraded = mini_render::StreamRenderer::new(MarkdownMode::Styled, false);
    out.push_str(&format!(
        "  styled without color degrades to plain: {}\n",
        degraded.push("**hi**\n") == "hi\n"
    ));
    let mut table = mini_render::StreamRenderer::new(MarkdownMode::Plain, false);
    let held = table.push("| area | status |\n") == String::new()
        && table.push("| --- | --- |\n") == String::new()
        && table.push("| auth | fixed |\n") == String::new();
    out.push_str(&format!("  unclosed table held back whole: {held}\n"));
    let drained = table.flush();
    out.push_str(&format!(
        "  table flush drains block at boundary: {}\n",
        drained.contains("area\nstatus\nauth\nfixed")
    ));

    out.push_str("\nescape stripping variants:\n");
    out.push_str(&format!(
        "  csi color stripped: {:?}\n",
        mini_render::strip_terminal_escapes("\x1b[31mred\x1b[0m ok")
    ));
    out.push_str(&format!(
        "  osc title with bel stripped: {:?}\n",
        mini_render::strip_terminal_escapes("a\x1b]0;title\x07b")
    ));
    out.push_str(&format!(
        "  osc hyperlink with st keeps text: {:?}\n",
        mini_render::strip_terminal_escapes("\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\")
    ));
    out.push_str(&format!(
        "  bare esc dropped: {:?}\n",
        mini_render::strip_terminal_escapes("a\x1bb")
    ));
    out.push_str(&format!(
        "  trailing partial sequence dropped: {:?}\n",
        mini_render::strip_terminal_escapes("abc\x1b[31")
    ));
    out.push_str(&format!(
        "  cr dropped, tab and newline kept: {:?}\n",
        mini_render::strip_terminal_escapes("a\rb\nc\td")
    ));

    out.push_str("\nline-buffered raw writer:\n");
    let mut writer = mini_render::AppendWriter::new();
    let buffered = writer.push("hello ") == String::new()
        && writer.push("world\ntail") == "hello world\n"
        && writer.take_remaining() == "tail";
    out.push_str(&format!("  complete lines only: {buffered}\n"));

    out.push_str("\nmarkdown mode parsing:\n");
    out.push_str(&format!(
        "  default is raw: {}\n",
        MarkdownMode::default() == MarkdownMode::Raw
    ));
    out.push_str(&format!(
        "  parses raw, plain, styled: {}\n",
        MarkdownMode::parse_opt("raw") == Some(MarkdownMode::Raw)
            && MarkdownMode::parse_opt(" PLAIN ") == Some(MarkdownMode::Plain)
            && MarkdownMode::parse_opt("Styled") == Some(MarkdownMode::Styled)
    ));
    out.push_str(&format!(
        "  rejects unknown values: {}\n",
        MarkdownMode::parse_opt("html").is_none()
    ));
    out.push_str(&format!(
        "  short names round-trip: {}\n",
        MarkdownMode::Raw.as_str() == "raw"
            && MarkdownMode::Plain.as_str() == "plain"
            && MarkdownMode::Styled.as_str() == "styled"
    ));
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("mini", "markdown_modes", &section());
        return;
    }
    println!("mini markdown: rendering branches of the wf-mini library");
    print!("{}", section());
}
