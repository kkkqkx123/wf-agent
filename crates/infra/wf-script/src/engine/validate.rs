use super::*;

use crate::error::ScriptError;
use crate::types::ScriptDefinition;

impl ScriptEngine {
    /// Require exactly one of `content` or `template` to be non-blank.
    /// Public so configuration validation shares the same rule instead of
    /// restating the shape check.
    pub fn validate_definition_shape(script: &ScriptDefinition) -> Result<(), ScriptError> {
        let has_content = script
            .content
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        let has_template = script
            .template
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        match (has_content, has_template) {
            (true, true) => Err(ScriptError::InvalidDefinition(format!(
                "Script '{}' defines both 'content' and 'template': pass exactly one",
                script.name
            ))),
            (false, false) => Err(ScriptError::InvalidDefinition(format!(
                "Script '{}' defines neither 'content' nor 'template': pass exactly one",
                script.name
            ))),
            _ => Ok(()),
        }
    }
}
