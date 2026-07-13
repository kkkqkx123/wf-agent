/**
 * Metrics System - Predefined Metric Names
 *
 * Standard metric name constants organized by category.
 * Provides type-safe metric names to prevent typos and ensure consistency.
 */

/**
 * Type for metric name constants
 */
export type MetricName = string;

/**
 * Standard metric names for workflow execution
 */
export const WORKFLOW_METRICS = {
  /** Total workflow executions (counter) */
  EXECUTION_COUNT: "workflow.execution.count" as const,
  /** Workflow execution duration in milliseconds (histogram) */
  EXECUTION_DURATION: "workflow.execution.duration" as const,
  /** Number of nodes in workflow (histogram) */
  NODE_COUNT: "workflow.node.count" as const,
  /** Successful workflow executions (counter) */
  SUCCESS_COUNT: "workflow.execution.success.count" as const,
  /** Failed workflow executions (counter) */
  FAILURE_COUNT: "workflow.execution.failure.count" as const,
  /** Active workflow executions (gauge) */
  ACTIVE_COUNT: "workflow.execution.active.count" as const,
  /** Error count by type (counter) */
  ERROR_COUNT: "workflow.error.count" as const,
  /** [Issue 9] Workflow-level retry count (counter) */
  RETRY_COUNT: "workflow.retry.count" as const,
  /** [Issue 9] Workflow-level retry delay time (histogram) */
  RETRY_DELAY_TIME: "workflow.retry.delay_time_ms" as const,
  /** [Issue 9] Workflow timeout count (counter) */
  TIMEOUT_COUNT: "workflow.timeout.count" as const,
} as const;

/**
 * Type for workflow metric names
 */
export type WorkflowMetricName = keyof typeof WORKFLOW_METRICS;

/**
 * Standard metric names for node execution
 */
export const NODE_METRICS = {
  /** Total node executions (counter) */
  EXECUTION_COUNT: "node.execution.count" as const,
  /** Node execution duration in milliseconds (histogram) */
  EXECUTION_DURATION: "node.execution.duration" as const,
  /** Successful node executions (counter) */
  SUCCESS_COUNT: "node.execution.success.count" as const,
  /** Failed node executions (counter) */
  FAILURE_COUNT: "node.execution.failure.count" as const,
  /** Node started count (counter) */
  STARTED_COUNT: "node.execution.started.count" as const,
  /** Retry count (counter) */
  RETRY_COUNT: "node.retry.count" as const,
  /** Error count by type (counter) */
  ERROR_COUNT: "node.error.count" as const,
  /** Input size in bytes (histogram) */
  INPUT_SIZE: "node.input.size" as const,
  /** Output size in bytes (histogram) */
  OUTPUT_SIZE: "node.output.size" as const,
  /** Token usage for LLM nodes (histogram) */
  TOKEN_USAGE: "node.execution.token_usage" as const,
} as const;

/**
 * Type for node metric names
 */
export type NodeMetricName = keyof typeof NODE_METRICS;

/**
 * Standard metric names for tool calls
 */
export const TOOL_METRICS = {
  CALL_DURATION: "tool.call.duration",
  CALL_COUNT: "tool.call.count",
  ERROR_COUNT: "tool.error.count",
  PARAMETER_SIZE: "tool.parameter.size",
  RESULT_SIZE: "tool.result.size",
} as const;

/**
 * Standard metric names for token usage
 */
export const TOKEN_METRICS = {
  TOTAL_TOKENS: "token.usage.total",
  PROMPT_TOKENS: "token.usage.prompt",
  COMPLETION_TOKENS: "token.usage.completion",
  COST: "token.cost.total",
  REQUEST_COUNT: "token.request.count",
} as const;

/**
 * Standard metric names for errors
 */
export const ERROR_METRICS = {
  OCCURRENCE_COUNT: "error.occurrence.count",
  RECOVERY_RATE: "error.recovery.rate",
  AFFECTED_EXECUTIONS: "error.affected.executions",
} as const;

/**
 * Standard metric names for resource utilization
 */
export const RESOURCE_METRICS = {
  MEMORY_USAGE: "resource.memory.usage",
  ACTIVE_EXECUTIONS: "resource.active.executions",
  QUEUED_TASKS: "resource.queued.tasks",
  EVENT_QUEUE_LENGTH: "resource.event.queue.length",
} as const;

/**
 * Standard metric names for agent loop execution
 */
export const AGENT_LOOP_METRICS = {
  // Execution lifecycle
  EXECUTION_DURATION: "agent_loop.execution.duration",
  EXECUTION_COUNT: "agent_loop.execution.count",
  ACTIVE_COUNT: "agent_loop.active.count",

  // Iteration statistics
  ITERATION_COUNT: "agent_loop.iteration.count",
  ITERATION_DURATION: "agent_loop.iteration.duration",
  MAX_ITERATIONS_REACHED: "agent_loop.iteration.limit_reached",

  // Tool calls
  TOOL_CALLS_TOTAL: "agent_loop.tool_calls.total",
  TOOL_CALLS_PER_ITERATION: "agent_loop.tool_calls.per_iteration",

  // State transitions
  PAUSE_COUNT: "agent_loop.pause.count",
  RESUME_COUNT: "agent_loop.resume.count",
  PAUSE_DURATION: "agent_loop.pause.duration",

  // Success rate
  SUCCESS_RATE: "agent_loop.success.rate",
  ERROR_COUNT: "agent_loop.error.count",
} as const;

/**
 * Standard metric names for template usage
 */
export const TEMPLATE_METRICS = {
  /** Template instantiation count (counter) */
  INSTANTIATION_COUNT: "node.template.instantiation.count" as const,
  /** Template render duration (histogram) */
  RENDER_DURATION: "template.render.duration" as const,
  /** Cache hit count (counter) */
  CACHE_HIT_COUNT: "template.cache.hit_count" as const,
  /** Cache miss count (counter) */
  CACHE_MISS_COUNT: "template.cache.miss_count" as const,
  /** Error count (counter) */
  ERROR_COUNT: "template.error.count" as const,
} as const;

/**
 * Type for template metric names
 */
export type TemplateMetricName = keyof typeof TEMPLATE_METRICS;

/**
 * Standard metric names for configuration access
 */
export const CONFIG_METRICS = {
  ACCESS_COUNT: "config.access.count",
  LOAD_DURATION: "config.load.duration",
  VALIDATION_ERROR_COUNT: "config.validation_error.count",
  CACHE_HIT_COUNT: "config.cache.hit_count",
  CACHE_MISS_COUNT: "config.cache.miss_count",
} as const;

/**
 * Standard metric names for SUBGRAPH execution
 */
export const SUBGRAPH_METRICS = {
  /** Total subgraph executions (counter) */
  EXECUTION_COUNT: "subgraph.execution.count" as const,
  /** Subgraph execution duration in milliseconds (histogram) */
  EXECUTION_DURATION: "subgraph.execution.duration" as const,
  /** Successful subgraph executions (counter) */
  SUCCESS_COUNT: "subgraph.execution.success.count" as const,
  /** Failed subgraph executions (counter) */
  FAILURE_COUNT: "subgraph.execution.failure.count" as const,
  /** Nested depth of subgraph (histogram) */
  NESTED_DEPTH: "subgraph.nested.depth" as const,
  /** Variable import count (counter) */
  VARIABLE_IMPORT_COUNT: "subgraph.variable.import.count" as const,
  /** Variable export count (counter) */
  VARIABLE_EXPORT_COUNT: "subgraph.variable.export.count" as const,
  /** Variable import duration in milliseconds (histogram) */
  VARIABLE_IMPORT_DURATION: "subgraph.variable.import.duration" as const,
  /** Variable export duration in milliseconds (histogram) */
  VARIABLE_EXPORT_DURATION: "subgraph.variable.export.duration" as const,
} as const;

/**
 * Type for subgraph metric names
 */
export type SubgraphMetricName = keyof typeof SUBGRAPH_METRICS;

/**
 * Standard metric names for retry and timeout operations
 */
export const RETRY_METRICS = {
  // Retry budget metrics
  BUDGET_CONSUMED_COUNT: "retry.budget.consumed.count",
  BUDGET_CONSUMED_TIME: "retry.budget.consumed.time_ms",
  BUDGET_REMAINING_COUNT: "retry.budget.remaining.count",
  BUDGET_REMAINING_TIME: "retry.budget.remaining.time_ms",
  BUDGET_EXHAUSTED: "retry.budget.exhausted.count",

  // Retry attempt metrics
  ATTEMPT_TOTAL: "retry.attempt.total",
  ATTEMPT_SUCCEEDED: "retry.attempt.succeeded",
  ATTEMPT_FAILED: "retry.attempt.failed",

  // Retry delay metrics
  DELAY_DURATION: "retry.delay.duration_ms",
  BACKOFF_FACTOR: "retry.backoff.factor",

  // Timeout error metrics
  TIMEOUT_ERROR_COUNT: "retry.timeout_error.count",
  TIMEOUT_ERROR_NO_RETRY: "retry.timeout_error.no_retry.count",

  // Retry outcome metrics
  ULTIMATELY_SUCCEEDED: "retry.outcome.succeeded",
  ULTIMATELY_FAILED: "retry.outcome.failed",

  // Per-consumer retry tracking
  CONSUMER_ACTIVE_RETRIES: "retry.consumer.active.count",
} as const;

/**
 * Type for retry metric names
 */
export type RetryMetricName = keyof typeof RETRY_METRICS;
