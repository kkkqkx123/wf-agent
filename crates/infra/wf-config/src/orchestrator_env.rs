//! Environment-variable overrides for assembled infrastructure configs.
//!
//! Declarative mapping from `WF_*` env vars to config fields. Kept separate
//! from the assembly orchestrator so the high-churn override list does not
//! obscure the load/preset flow.

use std::collections::HashMap;

use crate::env::{env_parse_bool, env_parse_int, EnvMappingBuilder, EnvValue};
use crate::error::{ConfigError, ConfigResult};

/// Parse a sandbox mode override. Unknown spellings fail fast so a typoed
/// `WF_SANDBOX_MODE` aborts startup instead of silently keeping the file
/// value. Prefix lists are deliberately not carried over the environment:
/// comma-separated parsing invites separator ambiguity, so large prefix
/// tables belong in the domain file.
fn env_parse_sandbox_mode(value: &str) -> ConfigResult<EnvValue> {
    match value.to_lowercase().as_str() {
        "disabled" => Ok(EnvValue::String("Disabled".to_string())),
        "lenient" => Ok(EnvValue::String("Lenient".to_string())),
        "strict" => Ok(EnvValue::String("Strict".to_string())),
        _ => Err(ConfigError::EnvVar(format!(
            "WF_SANDBOX_MODE must be one of disabled/lenient/strict, got '{value}'"
        ))),
    }
}

pub(crate) fn build_infra_env_mapping() -> HashMap<String, crate::env::EnvMappingEntry> {
    EnvMappingBuilder::new()
        .custom(
            "storage_type",
            "WF_STORAGE_TYPE",
            Box::new(|v| {
                let lower = v.to_lowercase();
                Ok(EnvValue::String(lower))
            }),
            None,
        )
        .string("storage_sqlite_db_path", "WF_STORAGE_SQLITE_DB_PATH", None)
        .custom(
            "limits_agent_max_iterations_cap",
            "WF_AGENT_MAX_ITERATIONS_CAP",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_agent_default_max_iterations",
            "WF_AGENT_DEFAULT_MAX_ITERATIONS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_agent_max_concurrent",
            "WF_AGENT_MAX_CONCURRENT",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_agent_max_sub_agent_depth",
            "WF_AGENT_MAX_SUB_AGENT_DEPTH",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_agent_max_pause_duration_ms",
            "WF_AGENT_MAX_PAUSE_DURATION_MS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_workflow_loop_max_iterations_cap",
            "WF_WORKFLOW_LOOP_MAX_ITERATIONS_CAP",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_workflow_loop_default_max_iterations",
            "WF_WORKFLOW_LOOP_DEFAULT_MAX_ITERATIONS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_workflow_max_navigation_multiplier",
            "WF_WORKFLOW_MAX_NAVIGATION_MULTIPLIER",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_exec_node_timeout_ms",
            "WF_EXEC_NODE_TIMEOUT_MS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "limits_exec_max_execution_time_ms",
            "WF_EXEC_MAX_EXECUTION_TIME_MS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "metrics_enabled",
            "WF_METRICS_ENABLED",
            Box::new(env_parse_bool),
            None,
        )
        .string("output_dir", "WF_OUTPUT_DIR", None)
        .custom(
            "file_checkpoint_enabled",
            "WF_FILE_CHECKPOINT_ENABLED",
            Box::new(env_parse_bool),
            None,
        )
        .string(
            "file_checkpoint_workspace_root",
            "WF_FILE_CHECKPOINT_WORKSPACE_ROOT",
            None,
        )
        .custom(
            "file_checkpoint_manual_watch",
            "WF_FILE_CHECKPOINT_MANUAL_WATCH",
            Box::new(env_parse_bool),
            None,
        )
        .custom(
            "file_checkpoint_manual_poll_ms",
            "WF_FILE_CHECKPOINT_MANUAL_POLL_MS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "file_checkpoint_gc_interval_secs",
            "WF_FILE_CHECKPOINT_GC_INTERVAL_SECS",
            Box::new(env_parse_int),
            None,
        )
        .custom(
            "sandbox_mode",
            "WF_SANDBOX_MODE",
            Box::new(env_parse_sandbox_mode),
            None,
        )
        .build()
}
