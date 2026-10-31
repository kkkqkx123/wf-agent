//! Limits configuration processor: merge user-provided resource limits with
//! defaults and validate them.
//!
//! Defaults mirror the current hard-coded constants in the engines:
//! agent iteration cap 1000, default agent iterations 10, sub-agent depth 8,
//! workflow loop cap 10000, default loop iterations 100, navigation
//! multiplier 5, node timeout fallback 30000ms, compression run timeout
//! 240000ms with 1 retry, a settle budget of 300000ms and tail_keep 2. The
//! terminal-failure fallback is not a global limit: it is declared per
//! compression workflow resource. `max_concurrent = 0` and
//! `max_pause_duration_ms = 0` mean "no explicit limit" and preserve the
//! engine's default behavior (CPU-core-derived concurrency, unbounded pause).

use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_min;

use wf_types::config::limits::{
    AgentLimits, CompactionLimits, CompressionLimits, ExecutionDefaults, LimitsConfig,
    WorkflowLimits,
};

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

pub const COMPRESSION_TAIL_KEEP_DEFAULT: usize = 2;
pub const COMPRESSION_MAX_RETRIES_DEFAULT: u32 = 1;
pub const COMPRESSION_TIMEOUT_MS_DEFAULT: u64 = 240_000;
pub const COMPRESSION_SETTLE_TIMEOUT_MS_DEFAULT: u64 = 300_000;

pub const COMPACTION_H_TARGET_RATIO_DEFAULT: f64 = 0.30;
pub const COMPACTION_T_HANDOFF_RATIO_DEFAULT: f64 = 0.55;
pub const COMPACTION_T_COMPACT_RATIO_DEFAULT: f64 = 0.70;
pub const COMPACTION_T_HARD_RATIO_DEFAULT: f64 = 0.85;
pub const COMPACTION_R_REPRESS_DEFAULT: u64 = 5000;
pub const COMPACTION_R_MIN_RANGE_DEFAULT: u64 = 2000;
pub const COMPACTION_K_REPRESS_DEFAULT: u32 = 2;
pub const COMPACTION_RECENT_RAW_TURNS_DEFAULT: usize = 2;
pub const COMPACTION_PLAN_MAX_ITEMS_DEFAULT: usize = 8;

/// Merge user limits with defaults, filling every absent field so the
/// returned config carries concrete values (never `None`).
pub fn merge_limits_with_defaults(user: &LimitsConfig) -> LimitsConfig {
    let user_agent = user.agent.as_ref();
    let user_workflow = user.workflow.as_ref();
    let user_exec = user.execution_defaults.as_ref();
    let user_compression = user.compression.as_ref();

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

    let compression = CompressionLimits {
        tail_keep: user_compression
            .and_then(|c| c.tail_keep)
            .or(Some(COMPRESSION_TAIL_KEEP_DEFAULT)),
        max_retries: user_compression
            .and_then(|c| c.max_retries)
            .or(Some(COMPRESSION_MAX_RETRIES_DEFAULT)),
        timeout_ms: user_compression
            .and_then(|c| c.timeout_ms)
            .or(Some(COMPRESSION_TIMEOUT_MS_DEFAULT)),
        settle_timeout_ms: user_compression
            .and_then(|c| c.settle_timeout_ms)
            .or(Some(COMPRESSION_SETTLE_TIMEOUT_MS_DEFAULT)),
    };

    let user_compaction = user.compaction.as_ref();
    let compaction = CompactionLimits {
        h_target_ratio: user_compaction
            .and_then(|c| c.h_target_ratio)
            .or(Some(COMPACTION_H_TARGET_RATIO_DEFAULT)),
        t_handoff_ratio: user_compaction
            .and_then(|c| c.t_handoff_ratio)
            .or(Some(COMPACTION_T_HANDOFF_RATIO_DEFAULT)),
        t_compact_ratio: user_compaction
            .and_then(|c| c.t_compact_ratio)
            .or(Some(COMPACTION_T_COMPACT_RATIO_DEFAULT)),
        t_hard_ratio: user_compaction
            .and_then(|c| c.t_hard_ratio)
            .or(Some(COMPACTION_T_HARD_RATIO_DEFAULT)),
        r_repress: user_compaction
            .and_then(|c| c.r_repress)
            .or(Some(COMPACTION_R_REPRESS_DEFAULT)),
        r_min_range: user_compaction
            .and_then(|c| c.r_min_range)
            .or(Some(COMPACTION_R_MIN_RANGE_DEFAULT)),
        k_repress: user_compaction
            .and_then(|c| c.k_repress)
            .or(Some(COMPACTION_K_REPRESS_DEFAULT)),
        recent_raw_turns: user_compaction
            .and_then(|c| c.recent_raw_turns)
            .or(Some(COMPACTION_RECENT_RAW_TURNS_DEFAULT)),
        tool_distill: user_compaction.and_then(|c| c.tool_distill).or(Some(true)),
        file_dedup: user_compaction.and_then(|c| c.file_dedup).or(Some(true)),
        plan_max_items: user_compaction
            .and_then(|c| c.plan_max_items)
            .or(Some(COMPACTION_PLAN_MAX_ITEMS_DEFAULT)),
        summary_call_budget_tokens: user_compaction.and_then(|c| c.summary_call_budget_tokens),
    };

    LimitsConfig {
        agent: Some(agent),
        workflow: Some(workflow),
        execution_defaults: Some(execution_defaults),
        compression: Some(compression),
        compaction: Some(compaction),
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
            validate_min(
                default_iter,
                1,
                "limits.workflow.loop_default_max_iterations",
            )?;
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
    if let Some(ref compression) = config.compression {
        if let Some(timeout) = compression.timeout_ms {
            validate_min(timeout, 1, "limits.compression.timeout_ms")?;
        }
        if let Some(settle) = compression.settle_timeout_ms {
            validate_min(settle, 1, "limits.compression.settle_timeout_ms")?;
        }
    }
    if let Some(ref compaction) = config.compaction {
        let ratios = [
            (
                "limits.compaction.h_target_ratio",
                compaction.h_target_ratio,
            ),
            (
                "limits.compaction.t_handoff_ratio",
                compaction.t_handoff_ratio,
            ),
            (
                "limits.compaction.t_compact_ratio",
                compaction.t_compact_ratio,
            ),
            ("limits.compaction.t_hard_ratio", compaction.t_hard_ratio),
        ];
        for (name, value) in ratios {
            match value {
                Some(r) if r > 0.0 && r < 1.0 => {}
                _ => {
                    return Err(ConfigError::Validation(format!(
                        "{name} must be within (0, 1)"
                    )));
                }
            }
        }
        let h = compaction.h_target_ratio.unwrap_or(0.0);
        let ho = compaction.t_handoff_ratio.unwrap_or(0.0);
        let c = compaction.t_compact_ratio.unwrap_or(0.0);
        let hard = compaction.t_hard_ratio.unwrap_or(0.0);
        if !(h < ho && ho < c && c < hard) {
            return Err(ConfigError::Validation(
                "limits.compaction ratios must strictly increase: h_target < t_handoff < t_compact < t_hard"
                    .to_string(),
            ));
        }
        if let Some(r_min) = compaction.r_min_range {
            validate_min(r_min, 1, "limits.compaction.r_min_range")?;
        }
        if let Some(r_repress) = compaction.r_repress {
            validate_min(r_repress, 1, "limits.compaction.r_repress")?;
        }
        if let (Some(r_min), Some(r_repress)) = (compaction.r_min_range, compaction.r_repress) {
            if r_min >= r_repress {
                return Err(ConfigError::Validation(
                    "limits.compaction.r_min_range must be smaller than r_repress".to_string(),
                ));
            }
        }
        if let Some(k) = compaction.k_repress {
            validate_min(k, 1, "limits.compaction.k_repress")?;
        }
        if let Some(turns) = compaction.recent_raw_turns {
            validate_min(turns as u64, 1, "limits.compaction.recent_raw_turns")?;
        }
        if let Some(max_items) = compaction.plan_max_items {
            validate_min(max_items as u64, 1, "limits.compaction.plan_max_items")?;
        }
        if let Some(budget) = compaction.summary_call_budget_tokens {
            validate_min(budget, 1, "limits.compaction.summary_call_budget_tokens")?;
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
        let compression = merged.compression.unwrap();
        assert_eq!(compression.tail_keep, Some(2));
        assert_eq!(compression.max_retries, Some(1));
        assert_eq!(compression.timeout_ms, Some(240_000));
        assert_eq!(compression.settle_timeout_ms, Some(300_000));
        let compaction = merged.compaction.unwrap();
        assert_eq!(compaction.h_target_ratio, Some(0.30));
        assert_eq!(compaction.t_handoff_ratio, Some(0.55));
        assert_eq!(compaction.t_compact_ratio, Some(0.70));
        assert_eq!(compaction.t_hard_ratio, Some(0.85));
        assert_eq!(compaction.r_repress, Some(5000));
        assert_eq!(compaction.r_min_range, Some(2000));
        assert_eq!(compaction.k_repress, Some(2));
        assert_eq!(compaction.recent_raw_turns, Some(2));
        assert_eq!(compaction.tool_distill, Some(true));
        assert_eq!(compaction.file_dedup, Some(true));
        assert_eq!(compaction.plan_max_items, Some(8));
        assert_eq!(compaction.summary_call_budget_tokens, None);
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
            compression: Some(CompressionLimits {
                tail_keep: Some(5),
                ..Default::default()
            }),
            ..Default::default()
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
    fn test_validate_compaction_rejects_unordered_ratios() {
        let bad = LimitsConfig {
            compaction: Some(CompactionLimits {
                h_target_ratio: Some(0.70),
                t_handoff_ratio: Some(0.55),
                t_compact_ratio: Some(0.70),
                t_hard_ratio: Some(0.85),
                r_repress: Some(5000),
                r_min_range: Some(2000),
                k_repress: Some(2),
                recent_raw_turns: Some(2),
                tool_distill: Some(true),
                file_dedup: Some(true),
                plan_max_items: Some(8),
                summary_call_budget_tokens: None,
            }),
            ..Default::default()
        };
        assert!(validate_limits_config(&bad).is_err());

        let bad_range = LimitsConfig {
            compaction: Some(CompactionLimits {
                h_target_ratio: Some(0.30),
                t_handoff_ratio: Some(0.55),
                t_compact_ratio: Some(0.70),
                t_hard_ratio: Some(0.85),
                r_repress: Some(2000),
                r_min_range: Some(5000),
                k_repress: Some(2),
                recent_raw_turns: Some(2),
                tool_distill: Some(true),
                file_dedup: Some(true),
                plan_max_items: Some(8),
                summary_call_budget_tokens: None,
            }),
            ..Default::default()
        };
        assert!(validate_limits_config(&bad_range).is_err());
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
