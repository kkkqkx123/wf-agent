//! Tool-call text protocol: parsing and prompt rendering.
pub mod parser;
pub mod protocol;

pub use parser::{ParseFormat, ToolCallParseOptions};
pub use protocol::{ToolProtocolTemplateSet, build_text_mode_system_content};
