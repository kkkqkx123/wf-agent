use super::*;

use std::collections::HashMap;

use crate::error::ScriptError;
use crate::payload as crate_payload;
use crate::types::{ScriptDefinition, ScriptExecutionOptions};

/// Uppercase `name` for `WF_INPUT_<NAME>` export, mapping every
/// non-alphanumeric byte to an underscore so the result is a valid env key.
fn sanitize_env_name(name: &str) -> String {
    name.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                b.to_ascii_uppercase() as char
            } else {
                '_'
            }
        })
        .collect()
}

impl ScriptEngine {
    /// Materialize and bound the payload channels before transport selection:
    /// `stdin_file` is read into `stdin` (capped), inline `stdin` is capped,
    /// the total environment size is capped, and every declared input file is
    /// validated and exported as `WF_INPUT_<NAME>` for the command to read.
    pub(super) async fn prepare_payload(
        script: &ScriptDefinition,
        merged: &mut ScriptExecutionOptions,
    ) -> Result<(), ScriptError> {
        if let Err(reason) = merged.validate_output_cap() {
            return Err(ScriptError::Payload(format!(
                "Script '{}': {reason}",
                script.name
            )));
        }
        if merged.stdin.is_some() && merged.stdin_file.is_some() {
            return Err(ScriptError::Payload(format!(
                "Script '{}' sets both 'stdin' and 'stdin_file': pass exactly one",
                script.name
            )));
        }
        if let Some(path) = merged.stdin_file.clone() {
            let content = tokio::fs::read(&path).await.map_err(|e| {
                ScriptError::Payload(format!(
                    "Script '{}' cannot read stdin file '{path}': {e}",
                    script.name
                ))
            })?;
            if content.len() > crate_payload::MAX_STDIN_BYTES {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' stdin file '{path}' ({} bytes) exceeds the {} byte limit: pass it as an input file instead",
                    script.name,
                    content.len(),
                    crate_payload::MAX_STDIN_BYTES
                )));
            }
            merged.stdin = Some(String::from_utf8(content).map_err(|_| {
                ScriptError::Payload(format!(
                    "Script '{}' stdin file '{path}' is not valid UTF-8",
                    script.name
                ))
            })?);
            merged.stdin_file = None;
        }
        if let Some(ref stdin) = merged.stdin {
            if stdin.len() > crate_payload::MAX_STDIN_BYTES {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' stdin ({} bytes) exceeds the {} byte limit: pass it as an input file instead",
                    script.name,
                    stdin.len(),
                    crate_payload::MAX_STDIN_BYTES
                )));
            }
        }
        if let Some(ref env) = merged.environment {
            let total = crate_payload::total_env_bytes(env);
            if total > crate_payload::MAX_ENV_BYTES {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' environment ({} bytes) exceeds the {} byte limit: pass large values as input files instead",
                    script.name,
                    total,
                    crate_payload::MAX_ENV_BYTES
                )));
            }
        }
        let Some(files) = merged.input_files.clone() else {
            return Ok(());
        };
        let workdir = merged.working_directory.as_deref();
        let env = merged.environment.get_or_insert_with(HashMap::new);
        let mut names: Vec<&String> = files.keys().collect();
        names.sort();
        let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in names {
            let path = &files[name];
            if let Err(reason) = crate_payload::validate_file_arg(path, workdir) {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' input file '{name}': {reason}",
                    script.name
                )));
            }
            // `WF_INPUT_*` is the script data plane (input file paths handed
            // to the child process), not application configuration: it is
            // never read by config resolution and must stay out of any
            // `WF_*` env-override unification.
            let var = format!("WF_INPUT_{}", sanitize_env_name(name));
            if !claimed.insert(var.clone()) {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' input file '{name}' collides after env sanitization: rename one of the colliding inputs",
                    script.name
                )));
            }
            if env.contains_key(&var) {
                return Err(ScriptError::Payload(format!(
                    "Script '{}' input file '{name}' overwrites existing environment '{var}': rename the input or the env key",
                    script.name
                )));
            }
            env.insert(var, path.clone());
        }
        Ok(())
    }
}
