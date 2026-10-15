//! Headless output plumbing shared by demos: format layer times sink layer.
//!
//! Business output goes to the sink (stdout); diagnostics go to the stderr
//! channel. Text streams deltas; structured formats take complete messages.

use wf_cli_shared::output::{HeadlessFileSink, OutputFormat, OutputSink};

/// Run raw chunk bytes through one text-compatible sink.
pub fn text_via_chunks(format: OutputFormat, chunks: &[&str]) -> String {
    let mut sink = HeadlessFileSink::buffer(Vec::new(), format, false);
    for chunk in chunks {
        sink.write_chunk(chunk).expect("write chunk");
    }
    sink.flush().expect("flush");
    String::from_utf8(sink.into_bytes()).expect("utf8 output")
}
