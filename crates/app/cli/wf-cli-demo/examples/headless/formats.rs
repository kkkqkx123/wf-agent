//! Headless formats demo: one write set through every sink and envelope.
//!
//! Shows how the same messages land in `text`, `json`, `jsonl`, and
//! `silent` sinks, how envelopes render per format, and what the memory
//! sink records.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example headless_formats
//! cargo run -p wf-cli-demo --example headless_formats -- --emit
//! ```

use wf_cli_demo::emit;
use wf_cli_shared::output::{
    HeadlessFileSink, MemorySink, OutputEnvelope, OutputFormat, OutputMessage, OutputSink,
};

fn section() -> String {
    let mut out = String::new();
    for format in [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::JsonLines,
        OutputFormat::Silent,
    ] {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), format, false);
        sink.write_message(&OutputMessage::new("user", "summarize main.rs"))
            .expect("write user");
        sink.write_message(&OutputMessage::new("assistant", "done"))
            .expect("write assistant");
        sink.write_text("free text line").expect("write text");
        sink.write_chunk("delta ").expect("write delta");
        sink.flush().expect("flush");
        let bytes = String::from_utf8(sink.into_bytes()).expect("utf8 output");
        out.push_str(&format!("format {format:?}:\n{bytes}\n"));
    }
    let ok = OutputEnvelope::success("execution", serde_json::json!({"executionId": "e1"}));
    let fail = OutputEnvelope::failure("execution", "boom");
    for format in [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::JsonLines,
        OutputFormat::Silent,
    ] {
        out.push_str(&format!(
            "ok {format:?}: {:?}\n",
            ok.render(format)
                .unwrap_or_else(|| "(no output)".to_string())
        ));
        out.push_str(&format!(
            "fail {format:?}: {:?}\n",
            fail.render(format)
                .unwrap_or_else(|| "(no output)".to_string())
        ));
    }
    let mut sink = MemorySink::new();
    sink.write_text("hello").expect("memory text");
    sink.write_message(&OutputMessage::new("assistant", "hi"))
        .expect("memory message");
    sink.write_chunk("wor").expect("memory chunk");
    sink.flush().expect("memory flush");
    out.push_str(&format!("memory text: {:?}\n", sink.text()));
    out.push_str(&format!("memory messages: {}\n", sink.messages().len()));
    out.push_str(&format!("memory flushes: {}\n", sink.flush_count()));
    out
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("headless", "formats", &section());
        return;
    }
    println!("headless formats: format layer times sink layer");
    print!("{}", section());
}
