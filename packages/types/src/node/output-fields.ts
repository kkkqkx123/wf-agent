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
 * The type constraint ensures that every field name listed is a valid key
 * of the corresponding node's output interface, preventing typos and
 * stale field references at compile time.
 */

import type { StartNodeOutput, EndNodeOutput } from "../workflow/boundary-config.js";
import type { VariableNodeOutput } from "./configs/variable-configs.js";
import type { ForkNodeOutput, JoinNodeOutput } from "./configs/fork-join-configs.js";
import type { SyncNodeOutput } from "./configs/sync-configs.js";
import type { SubgraphNodeOutput } from "./configs/subgraph-configs.js";
import type {
  ScriptNodeOutput,
  LLMNodeOutput,
  ToolVisibilityNodeOutput,
} from "./configs/execution-configs.js";
import type { UserInteractionNodeOutput } from "./configs/interaction-configs.js";
import type { RouteNodeOutput } from "./configs/control-configs.js";
import type { ContextProcessorNodeOutput } from "./configs/context-configs.js";
import type { LoopStartNodeOutput, LoopEndNodeOutput } from "./configs/loop-configs.js";
import type { AgentLoopNodeOutput } from "./configs/agent-loop-configs.js";

/**
 * Maps each runtime node type to its output shape interface.
 * Used solely to constrain DEFAULT_OUTPUT_FIELDS so that every field
 * name is guaranteed to be a valid key of the corresponding output type.
 */
type NodeOutputFieldMap = {
  START: StartNodeOutput;
  END: EndNodeOutput;
  VARIABLE: VariableNodeOutput;
  FORK: ForkNodeOutput;
  JOIN: JoinNodeOutput;
  SYNC: SyncNodeOutput;
  SUBGRAPH: SubgraphNodeOutput;
  SCRIPT: ScriptNodeOutput;
  LLM: LLMNodeOutput;
  TOOL_VISIBILITY: ToolVisibilityNodeOutput;
  USER_INTERACTION: UserInteractionNodeOutput;
  ROUTE: RouteNodeOutput;
  CONTEXT_PROCESSOR: ContextProcessorNodeOutput;
  LOOP_START: LoopStartNodeOutput;
  LOOP_END: LoopEndNodeOutput;
  AGENT_LOOP: AgentLoopNodeOutput;
  START_FROM_TRIGGER: StartNodeOutput;
  CONTINUE_FROM_TRIGGER: EndNodeOutput;
  EMBED_START: StartNodeOutput;
  EMBED_END: EndNodeOutput;
};

type OutputFieldConstraint = {
  [K in keyof NodeOutputFieldMap]: readonly (keyof NodeOutputFieldMap[K])[];
};

/**
 * Default fields to include in sanitized output for each node type.
 *
 * IMPORTANT: Each `as const` array is validated via `satisfies` below,
 * guaranteeing that every field name is a real key of the corresponding
 * *Output interface. If a handler's return shape changes, TypeScript
 * will flag any stale field names here.
 */
const _DEFAULT_OUTPUT_FIELDS = {
  START: ["input"] as const,
  END: ["output"] as const,
  VARIABLE: ["variableName", "oldValue", "newValue"] as const,
  FORK: ["launchedBranches"] as const,
  JOIN: [
    "completedBranches",
    "failedBranches",
    "skippedBranches",
    "strategy",
    "aggregatedOutput",
  ] as const,
  SYNC: ["syncedFromPath", "syncedVariables", "completed"] as const,
  SUBGRAPH: ["executionResult", "duration"] as const,
  SCRIPT: ["result"] as const,
  LLM: ["content", "toolCalls"] as const,
  TOOL_VISIBILITY: ["action", "toolIds"] as const,
  USER_INTERACTION: ["operationType", "userInput", "updatedVariables", "addedMessages"] as const,
  ROUTE: ["selectedRoute", "evaluatedConditions"] as const,
  CONTEXT_PROCESSOR: ["operationsApplied", "sourceContext", "targetContext"] as const,
  LOOP_START: ["loopId", "iterationCount", "maxIterations", "hasMoreIterations"] as const,
  LOOP_END: ["loopId", "breakTriggered", "iterationCount", "nextIteration"] as const,
  AGENT_LOOP: ["finalResponse", "toolCallCount", "iterationCount"] as const,
  START_FROM_TRIGGER: ["input"] as const,
  CONTINUE_FROM_TRIGGER: ["output"] as const,
  EMBED_START: ["message"] as const,
  EMBED_END: ["output"] as const,
} satisfies OutputFieldConstraint;

export const DEFAULT_OUTPUT_FIELDS: Record<string, readonly string[]> = _DEFAULT_OUTPUT_FIELDS;

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
