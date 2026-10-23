use super::*;

use regex::Regex;

use crate::error::ScriptError;
use crate::risk::RiskEvaluator;
use crate::types::{ScriptDefinition, ScriptSecurityPolicy};

impl ScriptEngine {
    /// Check script against the security policy before execution.
    /// Public so session-based paths gate on the same policy without
    /// duplicating the rule set. This is the early gate on the declared
    /// text; the rendered command is re-checked by `check_final_command`.
    pub fn check_security_policy(
        script: &ScriptDefinition,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), ScriptError> {
        if let Some(max_size) = policy.max_script_size {
            let content_len = script.content.as_deref().map(str::len).unwrap_or(0);
            let template_len = script.template.as_deref().map(str::len).unwrap_or(0);
            if content_len + template_len > max_size {
                return Err(ScriptError::PolicyDenied(format!(
                    "Script '{}' declared size ({} bytes) exceeds maximum allowed ({} bytes)",
                    script.name,
                    content_len + template_len,
                    max_size
                )));
            }
        }

        if let Some(ref allowed) = policy.allowed_languages {
            if let Some(ref lang) = script.language {
                if !allowed.iter().any(|a| a.eq_ignore_ascii_case(lang)) {
                    return Err(ScriptError::PolicyDenied(format!(
                        "Script '{}' language '{}' is not in allowed languages: [{}]",
                        script.name,
                        lang,
                        allowed.join(", ")
                    )));
                }
            }
        }

        let declared = script.content.as_deref().unwrap_or("").to_owned()
            + script.template.as_deref().unwrap_or("");
        Self::deny_text_patterns(&script.name, &declared, policy)?;

        if policy.allow_dynamic_scripts == Some(false)
            && script.content.is_some()
            && script.template.is_none()
        {
            return Err(ScriptError::PolicyDenied(format!(
                "Script '{}' is a dynamic script (runtime-generated) and dynamic scripts are not allowed",
                script.name
            )));
        }

        if policy.require_review == Some(true) {
            return Err(ScriptError::ReviewRequired(format!(
                "Script '{}' requires human review before execution",
                script.name
            )));
        }

        Ok(())
    }

    /// Mandatory gate on the rendered command. Argument interpolation can
    /// introduce blocked content that is invisible in the declared text,
    /// so the final command is checked even when the early gate passed.
    /// Risk scoring stays advisory; isolation is owned by the transport.
    pub fn check_final_command(
        script_name: &str,
        command: &str,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), ScriptError> {
        Self::deny_text_patterns(script_name, command, policy)?;
        if policy.require_review == Some(true) {
            return Err(ScriptError::ReviewRequired(format!(
                "Script '{script_name}' requires human review before execution"
            )));
        }
        Ok(())
    }

    pub(super) fn deny_text_patterns(
        script_name: &str,
        text: &str,
        policy: &ScriptSecurityPolicy,
    ) -> Result<(), ScriptError> {
        if let Some(ref blocked) = policy.blocked_patterns {
            for pattern in blocked {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(text) {
                            return Err(ScriptError::PolicyDenied(format!(
                                "Script '{script_name}' contains blocked pattern '{pattern}'"
                            )));
                        }
                    }
                    Err(e) => {
                        return Err(ScriptError::InvalidDefinition(format!(
                            "Invalid blocked pattern '{pattern}' in security policy: {e}"
                        )));
                    }
                }
            }
        }

        if let Some(ref forbidden) = policy.forbidden_commands {
            for cmd in forbidden {
                if Self::contains_command_token(text, cmd) {
                    return Err(ScriptError::PolicyDenied(format!(
                        "Script '{script_name}' contains forbidden command '{cmd}'"
                    )));
                }
            }
        }

        if let Some(ref path_patterns) = policy.forbidden_path_patterns {
            for pattern in path_patterns {
                match Regex::new(pattern) {
                    Ok(re) => {
                        if re.is_match(text) {
                            return Err(ScriptError::PolicyDenied(format!(
                                "Script '{script_name}' contains forbidden path pattern '{pattern}'"
                            )));
                        }
                    }
                    Err(e) => {
                        return Err(ScriptError::InvalidDefinition(format!(
                            "Invalid forbidden path pattern '{pattern}' in security policy: {e}"
                        )));
                    }
                }
            }
        }

        if let Some(ref max_risk) = policy.max_risk_level {
            let actual = RiskEvaluator::evaluate(text);
            if actual.rank() > max_risk.rank() {
                return Err(ScriptError::ReviewRequired(format!(
                    "Script '{script_name}' risk level {actual:?} exceeds maximum allowed {max_risk:?}: heuristic match gates to review, isolation stays with the transport"
                )));
            }
        }

        Ok(())
    }

    /// Token-bounded match so `rm` does not fire on `farm` while `rm -rf`
    /// still fires inside a longer command line.
    pub(super) fn contains_command_token(haystack: &str, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let is_boundary = |b: u8| !(b.is_ascii_alphanumeric() || b == b'_');
        haystack
            .as_bytes()
            .windows(needle.len())
            .enumerate()
            .any(|(i, window)| {
                window == needle.as_bytes()
                    && (i == 0 || is_boundary(haystack.as_bytes()[i - 1]))
                    && (i + needle.len() == haystack.len()
                        || is_boundary(haystack.as_bytes()[i + needle.len()]))
            })
    }
}
