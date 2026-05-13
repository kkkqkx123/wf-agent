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
  USAGE_COUNT: "template.usage.count",
  RENDER_DURATION: "template.render.duration",
  CACHE_HIT_COUNT: "template.cache.hit_count",
  CACHE_MISS_COUNT: "template.cache.miss_count",
  ERROR_COUNT: "template.error.count",
} as const;

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
