// Metric name constants, aligned with the TS SDK `packages/sdk/metrics/constants.ts`.

pub mod workflow_metrics {
    pub const EXECUTION_COUNT: &str = "workflow.execution.count";
    pub const EXECUTION_DURATION: &str = "workflow.execution.duration";
    pub const NODE_COUNT: &str = "workflow.node.count";
    pub const SUCCESS_COUNT: &str = "workflow.execution.success.count";
    pub const FAILURE_COUNT: &str = "workflow.execution.failure.count";
    pub const ACTIVE_COUNT: &str = "workflow.execution.active.count";
    pub const ERROR_COUNT: &str = "workflow.error.count";
    pub const RETRY_COUNT: &str = "workflow.retry.count";
    pub const RETRY_DELAY_TIME: &str = "workflow.retry.delay_time_ms";
    pub const TIMEOUT_COUNT: &str = "workflow.timeout.count";
}

pub mod node_metrics {
    pub const EXECUTION_COUNT: &str = "node.execution.count";
    pub const EXECUTION_DURATION: &str = "node.execution.duration";
    pub const SUCCESS_COUNT: &str = "node.execution.success.count";
    pub const FAILURE_COUNT: &str = "node.execution.failure.count";
    pub const STARTED_COUNT: &str = "node.execution.started.count";
    pub const RETRY_COUNT: &str = "node.retry.count";
    pub const ERROR_COUNT: &str = "node.error.count";
    pub const INPUT_SIZE: &str = "node.input.size";
    pub const OUTPUT_SIZE: &str = "node.output.size";
    pub const TOKEN_USAGE: &str = "node.execution.token_usage";
}

pub mod tool_metrics {
    pub const CALL_DURATION: &str = "tool.call.duration";
    pub const CALL_COUNT: &str = "tool.call.count";
    pub const ERROR_COUNT: &str = "tool.error.count";
    pub const PARAMETER_SIZE: &str = "tool.parameter.size";
    pub const RESULT_SIZE: &str = "tool.result.size";
}

pub mod token_metrics {
    pub const TOTAL_TOKENS: &str = "token.usage.total";
    pub const PROMPT_TOKENS: &str = "token.usage.prompt";
    pub const COMPLETION_TOKENS: &str = "token.usage.completion";
    pub const COST: &str = "token.cost.total";
    pub const REQUEST_COUNT: &str = "token.request.count";
}

pub mod error_metrics {
    pub const OCCURRENCE_COUNT: &str = "error.occurrence.count";
    pub const RECOVERY_RATE: &str = "error.recovery.rate";
    pub const AFFECTED_EXECUTIONS: &str = "error.affected.executions";
}

pub mod resource_metrics {
    pub const MEMORY_USAGE: &str = "resource.memory.usage";
    pub const ACTIVE_EXECUTIONS: &str = "resource.active.executions";
    pub const QUEUED_TASKS: &str = "resource.queued.tasks";
    pub const EVENT_QUEUE_LENGTH: &str = "resource.event.queue.length";
}

/// Storage I/O gauges recorded by the runtime resource sampler from the
/// `wf-storage` per-operation instrumentation.
pub mod storage_metrics {
    pub const OP_COUNT: &str = "storage.op.count";
    pub const OP_AVG_TIME_MS: &str = "storage.op.avg_time_ms";
    pub const OP_TOTAL_BYTES: &str = "storage.op.total_bytes";
}

pub mod agent_metrics {
    pub const EXECUTION_COUNT: &str = "agent.execution.count";
    pub const EXECUTION_DURATION: &str = "agent.execution.duration";
    pub const SUCCESS_COUNT: &str = "agent.execution.success.count";
    pub const FAILURE_COUNT: &str = "agent.execution.failure.count";
    pub const ITERATION_COUNT: &str = "agent.iteration.count";
    pub const TOOL_CALL_COUNT: &str = "agent.tool_call.count";
}

pub mod event_metrics {
    pub const EVENT_COUNT: &str = "event.count";
}

pub mod agent_loop_metrics {
    pub const EXECUTION_DURATION: &str = "agent_loop.execution.duration";
    pub const EXECUTION_COUNT: &str = "agent_loop.execution.count";
    pub const ACTIVE_COUNT: &str = "agent_loop.active.count";
    pub const ITERATION_COUNT: &str = "agent_loop.iteration.count";
    pub const ITERATION_DURATION: &str = "agent_loop.iteration.duration";
    pub const MAX_ITERATIONS_REACHED: &str = "agent_loop.iteration.limit_reached";
    pub const TOOL_CALLS_TOTAL: &str = "agent_loop.tool_calls.total";
    pub const TOOL_CALLS_PER_ITERATION: &str = "agent_loop.tool_calls.per_iteration";
    pub const PAUSE_COUNT: &str = "agent_loop.pause.count";
    pub const RESUME_COUNT: &str = "agent_loop.resume.count";
    pub const PAUSE_DURATION: &str = "agent_loop.pause.duration";
    pub const SUCCESS_RATE: &str = "agent_loop.success.rate";
    pub const ERROR_COUNT: &str = "agent_loop.error.count";
}

pub mod template_metrics {
    pub const INSTANTIATION_COUNT: &str = "node.template.instantiation.count";
    pub const RENDER_DURATION: &str = "template.render.duration";
    pub const CACHE_HIT_COUNT: &str = "template.cache.hit_count";
    pub const CACHE_MISS_COUNT: &str = "template.cache.miss_count";
    pub const ERROR_COUNT: &str = "template.error.count";
}

pub mod config_metrics {
    pub const ACCESS_COUNT: &str = "config.access.count";
    pub const LOAD_DURATION: &str = "config.load.duration";
    pub const VALIDATION_ERROR_COUNT: &str = "config.validation_error.count";
    pub const CACHE_HIT_COUNT: &str = "config.cache.hit_count";
    pub const CACHE_MISS_COUNT: &str = "config.cache.miss_count";
}

pub mod subgraph_metrics {
    pub const EXECUTION_COUNT: &str = "subgraph.execution.count";
    pub const EXECUTION_DURATION: &str = "subgraph.execution.duration";
    pub const SUCCESS_COUNT: &str = "subgraph.execution.success.count";
    pub const FAILURE_COUNT: &str = "subgraph.execution.failure.count";
    pub const NESTED_DEPTH: &str = "subgraph.nested.depth";
    pub const VARIABLE_IMPORT_COUNT: &str = "subgraph.variable.import.count";
    pub const VARIABLE_EXPORT_COUNT: &str = "subgraph.variable.export.count";
    pub const VARIABLE_IMPORT_DURATION: &str = "subgraph.variable.import.duration";
    pub const VARIABLE_EXPORT_DURATION: &str = "subgraph.variable.export.duration";
}

pub mod retry_metrics {
    pub const BUDGET_CONSUMED_COUNT: &str = "retry.budget.consumed.count";
    pub const BUDGET_CONSUMED_TIME: &str = "retry.budget.consumed.time_ms";
    pub const BUDGET_REMAINING_COUNT: &str = "retry.budget.remaining.count";
    pub const BUDGET_REMAINING_TIME: &str = "retry.budget.remaining.time_ms";
    pub const BUDGET_EXHAUSTED: &str = "retry.budget.exhausted.count";
    pub const ATTEMPT_TOTAL: &str = "retry.attempt.total";
    pub const ATTEMPT_SUCCEEDED: &str = "retry.attempt.succeeded";
    pub const ATTEMPT_FAILED: &str = "retry.attempt.failed";
    pub const DELAY_DURATION: &str = "retry.delay.duration_ms";
    pub const BACKOFF_FACTOR: &str = "retry.backoff.factor";
    pub const TIMEOUT_ERROR_COUNT: &str = "retry.timeout_error.count";
    pub const TIMEOUT_ERROR_NO_RETRY: &str = "retry.timeout_error.no_retry.count";
    pub const ULTIMATELY_SUCCEEDED: &str = "retry.outcome.succeeded";
    pub const ULTIMATELY_FAILED: &str = "retry.outcome.failed";
    pub const CONSUMER_ACTIVE_RETRIES: &str = "retry.consumer.active.count";
}

pub mod protocol_metrics {
    pub const LOCKED_COUNT: &str = "protocol.locked.count";
    pub const VIOLATION_COUNT: &str = "protocol.violation.count";
    pub const CONVERSION_COUNT: &str = "protocol.conversion.count";
    pub const STATIC_MISMATCH_COUNT: &str = "protocol.static_mismatch.count";
    pub const WORKFLOW_INCONSISTENCY_COUNT: &str = "protocol.workflow_inconsistency.count";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_unique_across_groups() {
        let mut seen = std::collections::HashSet::new();
        for group in [
            workflow_metrics::EXECUTION_COUNT,
            workflow_metrics::EXECUTION_DURATION,
            workflow_metrics::NODE_COUNT,
            workflow_metrics::SUCCESS_COUNT,
            workflow_metrics::FAILURE_COUNT,
            workflow_metrics::ACTIVE_COUNT,
            workflow_metrics::ERROR_COUNT,
            workflow_metrics::RETRY_COUNT,
            workflow_metrics::RETRY_DELAY_TIME,
            workflow_metrics::TIMEOUT_COUNT,
            node_metrics::EXECUTION_COUNT,
            node_metrics::EXECUTION_DURATION,
            node_metrics::SUCCESS_COUNT,
            node_metrics::FAILURE_COUNT,
            node_metrics::STARTED_COUNT,
            node_metrics::RETRY_COUNT,
            node_metrics::ERROR_COUNT,
            node_metrics::INPUT_SIZE,
            node_metrics::OUTPUT_SIZE,
            node_metrics::TOKEN_USAGE,
            tool_metrics::CALL_DURATION,
            tool_metrics::CALL_COUNT,
            tool_metrics::ERROR_COUNT,
            tool_metrics::PARAMETER_SIZE,
            tool_metrics::RESULT_SIZE,
            token_metrics::TOTAL_TOKENS,
            token_metrics::PROMPT_TOKENS,
            token_metrics::COMPLETION_TOKENS,
            token_metrics::COST,
            token_metrics::REQUEST_COUNT,
            error_metrics::OCCURRENCE_COUNT,
            error_metrics::RECOVERY_RATE,
            error_metrics::AFFECTED_EXECUTIONS,
            resource_metrics::MEMORY_USAGE,
            resource_metrics::ACTIVE_EXECUTIONS,
            resource_metrics::QUEUED_TASKS,
            resource_metrics::EVENT_QUEUE_LENGTH,
            agent_loop_metrics::EXECUTION_DURATION,
            agent_loop_metrics::EXECUTION_COUNT,
            agent_loop_metrics::ACTIVE_COUNT,
            agent_loop_metrics::ITERATION_COUNT,
            agent_loop_metrics::ITERATION_DURATION,
            agent_loop_metrics::MAX_ITERATIONS_REACHED,
            agent_loop_metrics::TOOL_CALLS_TOTAL,
            agent_loop_metrics::TOOL_CALLS_PER_ITERATION,
            agent_loop_metrics::PAUSE_COUNT,
            agent_loop_metrics::RESUME_COUNT,
            agent_loop_metrics::PAUSE_DURATION,
            agent_loop_metrics::SUCCESS_RATE,
            agent_loop_metrics::ERROR_COUNT,
            template_metrics::INSTANTIATION_COUNT,
            template_metrics::RENDER_DURATION,
            template_metrics::CACHE_HIT_COUNT,
            template_metrics::CACHE_MISS_COUNT,
            template_metrics::ERROR_COUNT,
            config_metrics::ACCESS_COUNT,
            config_metrics::LOAD_DURATION,
            config_metrics::VALIDATION_ERROR_COUNT,
            config_metrics::CACHE_HIT_COUNT,
            config_metrics::CACHE_MISS_COUNT,
            subgraph_metrics::EXECUTION_COUNT,
            subgraph_metrics::EXECUTION_DURATION,
            subgraph_metrics::SUCCESS_COUNT,
            subgraph_metrics::FAILURE_COUNT,
            subgraph_metrics::NESTED_DEPTH,
            subgraph_metrics::VARIABLE_IMPORT_COUNT,
            subgraph_metrics::VARIABLE_EXPORT_COUNT,
            subgraph_metrics::VARIABLE_IMPORT_DURATION,
            subgraph_metrics::VARIABLE_EXPORT_DURATION,
            retry_metrics::BUDGET_CONSUMED_COUNT,
            retry_metrics::BUDGET_CONSUMED_TIME,
            retry_metrics::BUDGET_REMAINING_COUNT,
            retry_metrics::BUDGET_REMAINING_TIME,
            retry_metrics::BUDGET_EXHAUSTED,
            retry_metrics::ATTEMPT_TOTAL,
            retry_metrics::ATTEMPT_SUCCEEDED,
            retry_metrics::ATTEMPT_FAILED,
            retry_metrics::DELAY_DURATION,
            retry_metrics::BACKOFF_FACTOR,
            retry_metrics::TIMEOUT_ERROR_COUNT,
            retry_metrics::TIMEOUT_ERROR_NO_RETRY,
            retry_metrics::ULTIMATELY_SUCCEEDED,
            retry_metrics::ULTIMATELY_FAILED,
            retry_metrics::CONSUMER_ACTIVE_RETRIES,
            protocol_metrics::LOCKED_COUNT,
            protocol_metrics::VIOLATION_COUNT,
            protocol_metrics::CONVERSION_COUNT,
            protocol_metrics::STATIC_MISMATCH_COUNT,
            protocol_metrics::WORKFLOW_INCONSISTENCY_COUNT,
        ] {
            assert!(seen.insert(group), "duplicate metric name: {group}");
        }
    }
}
