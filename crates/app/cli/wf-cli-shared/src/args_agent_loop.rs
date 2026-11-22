//! Detached agent-loop subcommand arguments (`AgentLoopSub`).

use clap::Subcommand;

/// Detached-run (keepalive) subcommands for agent loops.
#[derive(Debug, Clone, Subcommand)]
pub enum AgentLoopSub {
    /// Submit an agent loop run to the background and return its execution id
    /// immediately. The run keeps going after this command exits.
    Dispatch {
        /// Initial user message (prompt).
        #[arg(value_name = "PROMPT")]
        prompt: String,
        /// Agent loop definition id to dispatch. The server reuses the id as
        /// a session marker when `cli` is given.
        #[arg(long, default_value = "cli", value_name = "ID")]
        loop_id: String,
        /// Agent definition id (defaults to the built-in agent).
        #[arg(long)]
        agent: Option<String>,
        /// LLM profile id (defaults to `default`).
        #[arg(long)]
        model: Option<String>,
        /// Structured agent context as a JSON object (`--context '{"k":"v"}'`).
        #[arg(long, value_name = "JSON")]
        context: Option<String>,
        /// Server-side run budget in milliseconds.
        #[arg(long, value_name = "MS")]
        timeout: Option<u64>,
    },

    /// Attach to a (possibly finished) execution by id: replay retained
    /// history then follow live events until the terminal marker.
    Attach {
        /// Execution id to observe (the `execution_id` returned by dispatch).
        #[arg(value_name = "EXECUTION_ID")]
        execution_id: String,
        /// Replay cursor: last event timestamp seen by the client. Events at
        /// or older than it are skipped in the replay window.
        #[arg(long, value_name = "MS")]
        since: Option<String>,
    },
}
