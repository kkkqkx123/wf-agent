pub mod system_context;
pub mod user_context;

pub use system_context::{build_system_context, cleanup_empty_lines, wrap_section, SystemConfig};
pub use user_context::{build_user_context, UserInput};
