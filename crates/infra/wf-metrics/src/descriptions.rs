use crate::constants::*;

/// Central description table for Prometheus HELP lines.
///
/// Falls back to a humanized metric name when no entry exists, so newly
/// added metrics keep rendering without a table update.
pub fn metric_description(name: &str) -> Option<&'static str> {
    Some(match name {
        workflow_metrics::EXECUTION_COUNT => "Workflow execution count",
        workflow_metrics::EXECUTION_DURATION => "Workflow execution duration",
        workflow_metrics::NODE_COUNT => "Workflow node count",
        workflow_metrics::SUCCESS_COUNT => "Workflow execution success count",
        workflow_metrics::FAILURE_COUNT => "Workflow execution failure count",
        workflow_metrics::ACTIVE_COUNT => "Workflow active execution count",
        workflow_metrics::ERROR_COUNT => "Workflow error count",
        workflow_metrics::RETRY_COUNT => "Workflow retry count",
        workflow_metrics::RETRY_DELAY_TIME => "Workflow retry delay time",
        workflow_metrics::TIMEOUT_COUNT => "Workflow timeout count",
        node_metrics::EXECUTION_COUNT => "Node execution count",
        node_metrics::EXECUTION_DURATION => "Node execution duration",
        node_metrics::SUCCESS_COUNT => "Node execution success count",
        node_metrics::FAILURE_COUNT => "Node execution failure count",
        node_metrics::STARTED_COUNT => "Node execution started count",
        node_metrics::RETRY_COUNT => "Node retry count",
        node_metrics::ERROR_COUNT => "Node error count",
        node_metrics::INPUT_SIZE => "Node input size",
        node_metrics::OUTPUT_SIZE => "Node output size",
        node_metrics::TOKEN_USAGE => "Node execution token usage",
        tool_metrics::CALL_DURATION => "Tool call duration",
        tool_metrics::CALL_COUNT => "Tool call count",
        tool_metrics::ERROR_COUNT => "Tool error count",
        tool_metrics::PARAMETER_SIZE => "Tool parameter size",
        tool_metrics::RESULT_SIZE => "Tool result size",
        tool_metrics::GENERAL_INVOKE_COUNT => "Tool general invoke count",
        tool_metrics::GENERAL_INVOKE_DURATION => "Tool general invoke duration",
        tool_metrics::DISCOVERY_COUNT => "Tool discovery count",
        tool_metrics::ACTIVATION_COUNT => "Tool activation count",
        token_metrics::TOTAL_TOKENS => "Token usage total",
        token_metrics::PROMPT_TOKENS => "Token usage prompt",
        token_metrics::COMPLETION_TOKENS => "Token usage completion",
        token_metrics::COST => "Token cost total",
        token_metrics::REQUEST_COUNT => "Token request count",
        error_metrics::OCCURRENCE_COUNT => "Error occurrence count",
        error_metrics::RECOVERY_RATE => "Error recovery rate",
        error_metrics::AFFECTED_EXECUTIONS => "Error affected executions",
        resource_metrics::MEMORY_USAGE => "Resource memory usage",
        resource_metrics::ACTIVE_EXECUTIONS => "Resource active executions",
        resource_metrics::QUEUED_TASKS => "Resource queued tasks",
        resource_metrics::EVENT_QUEUE_LENGTH => "Resource event queue length",
        storage_metrics::OP_COUNT => "Storage operation count",
        storage_metrics::OP_AVG_TIME_MS => "Storage operation average time",
        storage_metrics::OP_TOTAL_BYTES => "Storage operation total bytes",
        agent_metrics::EXECUTION_COUNT => "Agent execution count",
        agent_metrics::EXECUTION_DURATION => "Agent execution duration",
        agent_metrics::SUCCESS_COUNT => "Agent execution success count",
        agent_metrics::FAILURE_COUNT => "Agent execution failure count",
        agent_metrics::ITERATION_COUNT => "Agent iteration count",
        agent_metrics::TOOL_CALL_COUNT => "Agent tool call count",
        event_metrics::EVENT_COUNT => "Event count",
        agent_loop_metrics::EXECUTION_DURATION => "Agent loop execution duration",
        agent_loop_metrics::EXECUTION_COUNT => "Agent loop execution count",
        agent_loop_metrics::ACTIVE_COUNT => "Agent loop active count",
        agent_loop_metrics::ITERATION_COUNT => "Agent loop iteration count",
        agent_loop_metrics::ITERATION_DURATION => "Agent loop iteration duration",
        agent_loop_metrics::MAX_ITERATIONS_REACHED => "Agent loop iteration limit reached",
        agent_loop_metrics::TOOL_CALLS_TOTAL => "Agent loop tool calls total",
        agent_loop_metrics::TOOL_CALLS_PER_ITERATION => "Agent loop tool calls per iteration",
        agent_loop_metrics::PAUSE_COUNT => "Agent loop pause count",
        agent_loop_metrics::RESUME_COUNT => "Agent loop resume count",
        agent_loop_metrics::PAUSE_DURATION => "Agent loop pause duration",
        agent_loop_metrics::SUCCESS_RATE => "Agent loop success rate",
        agent_loop_metrics::ERROR_COUNT => "Agent loop error count",
        config_metrics::ACCESS_COUNT => "Config access count",
        config_metrics::LOAD_DURATION => "Config load duration",
        config_metrics::VALIDATION_ERROR_COUNT => "Config validation error count",
        config_metrics::CACHE_HIT_COUNT => "Config cache hit count",
        config_metrics::CACHE_MISS_COUNT => "Config cache miss count",
        subgraph_metrics::EXECUTION_COUNT => "Subgraph execution count",
        subgraph_metrics::EXECUTION_DURATION => "Subgraph execution duration",
        subgraph_metrics::SUCCESS_COUNT => "Subgraph execution success count",
        subgraph_metrics::FAILURE_COUNT => "Subgraph execution failure count",
        subgraph_metrics::NESTED_DEPTH => "Subgraph nested depth",
        subgraph_metrics::VARIABLE_IMPORT_COUNT => "Subgraph variable import count",
        subgraph_metrics::VARIABLE_EXPORT_COUNT => "Subgraph variable export count",
        subgraph_metrics::VARIABLE_IMPORT_DURATION => "Subgraph variable import duration",
        subgraph_metrics::VARIABLE_EXPORT_DURATION => "Subgraph variable export duration",
        retry_metrics::BUDGET_CONSUMED_COUNT => "Retry budget consumed count",
        retry_metrics::BUDGET_CONSUMED_TIME => "Retry budget consumed time",
        retry_metrics::BUDGET_REMAINING_COUNT => "Retry budget remaining count",
        retry_metrics::BUDGET_REMAINING_TIME => "Retry budget remaining time",
        retry_metrics::BUDGET_EXHAUSTED => "Retry budget exhausted count",
        retry_metrics::ATTEMPT_TOTAL => "Retry attempt total",
        retry_metrics::ATTEMPT_SUCCEEDED => "Retry attempt succeeded",
        retry_metrics::ATTEMPT_FAILED => "Retry attempt failed",
        retry_metrics::DELAY_DURATION => "Retry delay duration",
        retry_metrics::BACKOFF_FACTOR => "Retry backoff factor",
        retry_metrics::TIMEOUT_ERROR_COUNT => "Retry timeout error count",
        retry_metrics::TIMEOUT_ERROR_NO_RETRY => "Retry timeout error no retry count",
        retry_metrics::ULTIMATELY_SUCCEEDED => "Retry outcome succeeded",
        retry_metrics::ULTIMATELY_FAILED => "Retry outcome failed",
        retry_metrics::CONSUMER_ACTIVE_RETRIES => "Retry consumer active count",
        protocol_metrics::LOCKED_COUNT => "Protocol locked count",
        protocol_metrics::VIOLATION_COUNT => "Protocol violation count",
        protocol_metrics::CONVERSION_COUNT => "Protocol conversion count",
        protocol_metrics::STATIC_MISMATCH_COUNT => "Protocol static mismatch count",
        protocol_metrics::WORKFLOW_INCONSISTENCY_COUNT => "Protocol workflow inconsistency count",
        template_metrics::INSTANTIATION_COUNT => "Node template instantiation count",
        template_metrics::RENDER_DURATION => "Template render duration",
        template_metrics::CACHE_HIT_COUNT => "Template cache hit count",
        template_metrics::CACHE_MISS_COUNT => "Template cache miss count",
        template_metrics::ERROR_COUNT => "Template error count",
        timeout_metrics::REGISTRATION_COUNT => "Timeout registration count",
        timeout_metrics::DURATION_CONFIGURED => "Timeout duration configured",
        timeout_metrics::EXPIRATION_COUNT => "Timeout expiration count",
        timeout_metrics::DURATION_ACTUAL => "Timeout duration actual",
        timeout_metrics::CANCELLATION_COUNT => "Timeout cancellation count",
        timeout_metrics::WARNING_COUNT => "Timeout warning count",
        timeout_metrics::WARNING_REMAINING_TIME => "Timeout warning remaining time",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_metrics_have_descriptions() {
        assert_eq!(
            metric_description(crate::constants::workflow_metrics::EXECUTION_COUNT),
            Some("Workflow execution count")
        );
        assert_eq!(
            metric_description(crate::constants::event_metrics::EVENT_COUNT),
            Some("Event count")
        );
        assert_eq!(
            metric_description(crate::constants::error_metrics::OCCURRENCE_COUNT),
            Some("Error occurrence count")
        );
    }

    #[test]
    fn unknown_metrics_fall_back() {
        assert_eq!(metric_description("custom.metric"), None);
    }
}
