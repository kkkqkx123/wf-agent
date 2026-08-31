//! Limits configuration processor: merge user-provided resource limits with
//! defaults and validate them.
//!
//! Defaults mirror the current hard-coded constants in the engines:
//! agent iteration cap 1000, default agent iterations 10, sub-agent depth 8,
//! workflow loop cap 10000, default loop iterations 100, navigation
//! multiplier 5, node timeout fallback 30000ms. `max_concurrent = 0` and
//! `max_pause_duration_ms = 0` mean "no explicit limit" and preserve the
//! engine's default behavior (CPU-core-derived concurrency, unbounded pause).

use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_min;

use wf_types::config::limits::{AgentLimits, ExecutionDefaults, LimitsConfig, WorkflowLimits};

pub const AGENT_MAX_ITERATIONS_CAP_DEFAULT: u32 = 1000;
pub const AGENT_DEFAULT_MAX_ITERATIONS_DEFAULT: u32 = 10;
pub const AGENT_MAX_CONCURRENT_DEFAULT: u32 = 0;
pub const AGENT_MAX_SUB_AGENT_DEPTH_DEFAULT: u32 = 8;
pub const AGENT_MAX_PAUSE_DURATION_MS_DEFAULT: u64 = 0;

pub const WORKFLOW_LOOP_MAX_ITERATIONS_CAP_DEFAULT: u32 = 10_000;
pub const WORKFLOW_LOOP_DEFAULT_MAX_ITERATIONS_DEFAULT: u32 = 100;
pub const WORKFLOW_MAX_NAVIGATION_MULTIPLIER_DEFAULT: u32 = 5;
pub const WORKFLOW_MAX_CONCURRENT_DEFAULT: u32 = 0;

pub const EXEC_NODE_TIMEOUT_MS_DEFAULT: u64 = 30_000;
pub const EXEC_MAX_EXECUTION_TIME_MS_DEFAULT: u64 = 0;

/// Merge user limits with defaults, filling every absent field so the
/// returned config carries concrete values (never `None`).
pub fn merge_limits_with_defaults(user: &LimitsConfig) -> LimitsConfig {
    let user_agent = user.agent.as_ref();
    let user_workflow = user.workflow.as_ref();
    let user_exec = user.execution_defaults.as_ref();

    let agent = AgentLimits {
        max_iterations_cap: user_agent
            .and_then(|a| a.max_iterations_cap)
            .or(Some(AGENT_MAX_ITERATIONS_CAP_DEFAULT)),
        default_max_iterations: user_agent
            .and_then(|a| a.default_max_iterations)
            .or(Some(AGENT_DEFAULT_MAX_ITERATIONS_DEFAULT)),
        max_concurrent: user_agent
            .and_then(|a| a.max_concurrent)
            .or(Some(AGENT_MAX_CONCURRENT_DEFAULT)),
        max_sub_agent_depth: user_agent
            .and_then(|a| a.max_sub_agent_depth)
            .or(Some(AGENT_MAX_SUB_AGENT_DEPTH_DEFAULT)),
        max_pause_duration_ms: user_agent
            .and_then(|a| a.max_pause_duration_ms)
            .or(Some(AGENT_MAX_PAUSE_DURATION_MS_DEFAULT)),
    };

    let workflow = WorkflowLimits {
        loop_max_iterations_cap: user_workflow
            .and_then(|w| w.loop_max_iterations_cap)
            .or(Some(WORKFLOW_LOOP_MAX_ITERATIONS_CAP_DEFAULT)),
        loop_default_max_iterations: user_workflow
            .and_then(|w| w.loop_default_max_iterations)
            .or(Some(WORKFLOW_LOOP_DEFAULT_MAX_ITERATIONS_DEFAULT)),
        max_navigation_multiplier: user_workflow
            .and_then(|w| w.max_navigation_multiplier)
            .or(Some(WORKFLOW_MAX_NAVIGATION_MULTIPLIER_DEFAULT)),
        max_concurrent: user_workflow
            .and_then(|w| w.max_concurrent)
            .or(Some(WORKFLOW_MAX_CONCURRENT_DEFAULT)),
    };

    let execution_defaults = ExecutionDefaults {
        node_timeout_ms: user_exec
            .and_then(|e| e.node_timeout_ms)
            .or(Some(EXEC_NODE_TIMEOUT_MS_DEFAULT)),
        max_execution_time_ms: user_exec
            .and_then(|e| e.max_execution_time_ms)
            .or(Some(EXEC_MAX_EXECUTION_TIME_MS_DEFAULT)),
    };

    LimitsConfig {
        agent: Some(agent),
        workflow: Some(workflow),
        execution_defaults: Some(execution_defaults),
    }
}

/// Validate merged limits: caps and multipliers must be positive; a
/// zero-valued field is only allowed where it means "no explicit limit"
/// (concurrency and pause duration).
pub fn validate_limits_config(config: &LimitsConfig) -> ConfigResult<()> {
    if let Some(ref agent) = config.agent {
        if let Some(cap) = agent.max_iterations_cap {
            validate_min(cap, 1, "limits.agent.max_iterations_cap")?;
        }
        if let Some(default_iter) = agent.default_max_iterations {
            validate_min(default_iter, 1, "limits.agent.default_max_iterations")?;
            if let Some(cap) = agent.max_iterations_cap {
                if default_iter > cap {
                    return Err(ConfigError::Validation(format!(
                        "limits.agent.default_max_iterations ({default_iter}) exceeds \
                         limits.agent.max_iterations_cap ({cap})"
                    )));
                }
            }
        }
    }
    if let Some(ref workflow) = config.workflow {
        if let Some(cap) = workflow.loop_max_iterations_cap {
            validate_min(cap, 1, "limits.workflow.loop_max_iterations_cap")?;
        }
        if let Some(default_iter) = workflow.loop_default_max_iterations {
            validate_min(default_iter, 1, "limits.workflow.loop_default_max_iterations")?;
            if let Some(cap) = workflow.loop_max_iterations_cap {
                if default_iter > cap {
                    return Err(ConfigError::Validation(format!(
                        "limits.workflow.loop_default_max_iterations ({default_iter}) exceeds \
                         limits.workflow.loop_max_iterations_cap ({cap})"
                    )));
                }
            }
        }
        if let Some(multiplier) = workflow.max_navigation_multiplier {
            validate_min(multiplier, 1, "limits.workflow.max_navigation_multiplier")?;
        }
    }
    if let Some(ref exec) = config.execution_defaults {
        if let Some(node_timeout) = exec.node_timeout_ms {
            if node_timeout == 0 {
                return Err(ConfigError::Validation(
                    "limits.execution_defaults.node_timeout_ms must be at least 1, got 0"
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_limits_fills_all_defaults() {
        let merged = merge_limits_with_defaults(&LimitsConfig::default());
        let agent = merged.agent.unwrap();
        assert_eq!(agent.max_iterations_cap, Some(1000));
        assert_eq!(agent.default_max_iterations, Some(10));
        assert_eq!(agent.max_concurrent, Some(0));
        assert_eq!(agent.max_sub_agent_depth, Some(8));
        assert_eq!(agent.max_pause_duration_ms, Some(0));
        let workflow = merged.workflow.unwrap();
        assert_eq!(workflow.loop_max_iterations_cap, Some(10000));
        assert_eq!(workflow.loop_default_max_iterations, Some(100));
        assert_eq!(workflow.max_navigation_multiplier, Some(5));
        assert_eq!(workflow.max_concurrent, Some(0));
        let exec = merged.execution_defaults.unwrap();
        assert_eq!(exec.node_timeout_ms, Some(30000));
        assert_eq!(exec.max_execution_time_ms, Some(0));
    }

    #[test]
    fn test_merge_limits_keeps_user_values() {
        let user = LimitsConfig {
            agent: Some(AgentLimits {
                max_iterations_cap: Some(500),
                default_max_iterations: Some(20),
                ..Default::default()
            }),
            workflow: Some(WorkflowLimits {
                loop_max_iterations_cap: Some(5000),
                ..Default::default()
            }),
            execution_defaults: Some(ExecutionDefaults {
                node_timeout_ms: Some(60000),
                ..Default::default()
            }),
        };
        let merged = merge_limits_with_defaults(&user);
        let agent = merged.agent.clone().unwrap();
        let workflow = merged.workflow.clone().unwrap();
        let defaults = merged.execution_defaults.clone().unwrap();
        assert_eq!(agent.max_iterations_cap, Some(500));
        assert_eq!(agent.default_max_iterations, Some(20));
        assert_eq!(workflow.loop_max_iterations_cap, Some(5000));
        assert_eq!(defaults.node_timeout_ms, Some(60000));
    }

    #[test]
    fn test_validate_limits_rejects_invalid() {
        let bad = LimitsConfig {
            agent: Some(AgentLimits {
                max_iterations_cap: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_limits_config(&bad).is_err());

        let bad = LimitsConfig {
            workflow: Some(WorkflowLimits {
                loop_max_iterations_cap: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_limits_config(&bad).is_err());

        let bad = LimitsConfig {
            workflow: Some(WorkflowLimits {
                max_navigation_multiplier: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_limits_config(&bad).is_err());

        let bad = LimitsConfig {
            agent: Some(AgentLimits {
                max_iterations_cap: Some(10),
                default_max_iterations: Some(20),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_limits_config(&bad).is_err());
    }

    #[test]
    fn test_validate_limits_accepts_valid() {
        let merged = merge_limits_with_defaults(&LimitsConfig::default());
        assert!(validate_limits_config(&merged).is_ok());
    }
}
