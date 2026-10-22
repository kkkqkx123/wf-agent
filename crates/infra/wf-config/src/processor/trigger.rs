use std::collections::HashMap;

use crate::error::{ConfigError, ConfigResult};
use crate::processor::substitute::substitute_in_struct;
use crate::validator::{validate_min, validate_not_empty};

use wf_types::trigger::config::TriggerAction;
use wf_types::trigger::template::{TriggerRuntimeLimits, TriggerTemplate};

mod actions;
mod limits;
mod scopes;
mod template;
mod transform;

pub use actions::*;
pub use limits::*;
pub use scopes::*;
pub use template::*;
pub use transform::*;


#[cfg(test)]
#[path = "trigger/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "trigger/tests_b.rs"]
mod tests_b;
