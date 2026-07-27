pub mod check;
pub mod handler;
pub mod state;

pub use check::check_execution_interruption;
pub use handler::execute_with_interruption_handling;
pub use state::{InterruptionSignal, InterruptionState};
