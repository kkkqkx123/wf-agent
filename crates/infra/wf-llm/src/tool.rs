// Tool call protocol: text-mode parsing and prompt rendering. The canonical
// implementation lives in the shared llm-kit crate; wf-llm re-exports it so
// existing `wf_llm::tool::{parser, protocol}` paths keep working.
pub use llm_tool_call::tool::{parser, protocol};
