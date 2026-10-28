//! Reserved variable keys of the user-interaction protocol.
//!
//! USER_INTERACTION and INTERACTIVE_SCRIPT nodes coordinate through the
//! workflow variable map: synchronous presets arrive before execution and
//! asynchronous responses land while the node waits. The keys stay in the
//! variable map (not in the live interaction registry) so presets survive
//! checkpoint resume; the single predicate in `wf_execution_shared::context`
//! keeps user writes away from them.

/// Response delivered to a USER_INTERACTION node: either a synchronous
/// preset read before waiting or the asynchronously resolved value.
pub const RESPONSE_VAR: &str = "__interaction_response__";

/// Transient marker while a USER_INTERACTION node waits for an external
/// response; cleared when the wait lands.
pub const WAITING_VAR: &str = "__interaction_waiting__";

/// Preset user input consumed by INTERACTIVE_SCRIPT nodes before a session
/// starts.
pub const INPUT_VAR: &str = "__interaction_input__";

/// Parsed output published by an INTERACTIVE_SCRIPT session for downstream
/// nodes.
pub const OUTPUT_VAR: &str = "__interaction_output__";
