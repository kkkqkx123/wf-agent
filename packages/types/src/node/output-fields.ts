/**
 * Runtime Output Field Configuration
 *
 * DEFAULT_OUTPUT_FIELDS defines the default set of fields to include
 * in the sanitized output of each node type. This is used by the
 * sanitizeNodeOutput function at runtime.
 *
 * Each entry lists field names that should be preserved from the raw
 * handler output. Fields not listed (or not matched) are treated as
 * internal implementation details and stripped.
 *
 * Nodes can override these defaults using NodeOutputConfig.includeFields.
 *
 * NOTE: These field names should correspond to the keys of the _output
 * type annotation defined in each node's config interface.
 */

/**
 * Default fields to include in sanitized output for each node type.
 */
export const DEFAULT_OUTPUT_FIELDS: Record<string, string[]> = {
  START: ["startedAt"],
  END: ["output"],
  VARIABLE: ["variableName", "oldValue", "newValue"],
  FORK: ["launchedBranches"],
  JOIN: ["completedBranches", "failedBranches", "skippedBranches", "strategy", "aggregatedOutput"],
  SYNC: ["syncedFromPath", "syncedVariables", "completed"],
  SUBGRAPH: ["executionResult", "duration"],
  SCRIPT: ["result"],
  LLM: ["content", "toolCalls"],
  TOOL_VISIBILITY: ["action", "toolIds"],
  USER_INTERACTION: ["operationType", "userInput", "updatedVariables", "addedMessages"],
  ROUTE: ["selectedRoute", "evaluatedConditions"],
  CONTEXT_PROCESSOR: ["operationsApplied", "sourceContext", "targetContext"],
  LOOP_START: ["loopId", "iterationCount", "maxIterations", "hasMoreIterations"],
  LOOP_END: ["loopId", "breakTriggered", "iterationCount", "nextIteration"],
  AGENT_LOOP: ["finalResponse", "toolCallCount", "iterationCount"],
  START_FROM_TRIGGER: ["startedAt"],
  CONTINUE_FROM_TRIGGER: ["output"],
  EMBED_START: ["startedAt"],
  EMBED_END: ["output"],
};

/**
 * Node output configuration section
 * Each node can optionally define how its output should be shaped
 */
export interface NodeOutputConfig {
  /**
   * Semantic output ID for referencing this node's output in expressions
   * Used in condition expressions like: node.<outputId>.field
   */
  outputId?: string;

  /**
   * List of field names to include in the sanitized output.
   * When not specified, uses the default fields for the node type (DEFAULT_OUTPUT_FIELDS).
   * When specified, only these fields are preserved.
   */
  includeFields?: string[];

  /**
   * Whether to exclude internal metadata fields automatically.
   * Defaults to true.
   */
  excludeInternal?: boolean;
}