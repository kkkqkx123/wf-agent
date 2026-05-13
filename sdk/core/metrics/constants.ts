/**
 * Metrics System - Predefined Metric Names
 * 
 * Standard metric name constants organized by category.
 */

/**
 * Standard metric names for workflow execution
 */
export const WORKFLOW_METRICS = {
  EXECUTION_DURATION: "workflow.execution.duration",
  EXECUTION_COUNT: "workflow.execution.count",
  NODE_COUNT: "workflow.node.count",
  ERROR_COUNT: "workflow.error.count",
  SUCCESS_RATE: "workflow.success.rate",
} as const;

/**
 * Standard metric names for node execution
 */
export const NODE_METRICS = {
  EXECUTION_DURATION: "node.execution.duration",
  EXECUTION_COUNT: "node.execution.count",
  RETRY_COUNT: "node.retry.count",
  ERROR_COUNT: "node.error.count",
  INPUT_SIZE: "node.input.size",
  OUTPUT_SIZE: "node.output.size",
} as const;

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
