/**
 * Thread Definition Type
 */

import type { ID, Version } from "../common.js";
import type { WorkflowGraph } from "../graph/index.js";
import type { WorkflowExecutionType } from "./status.js";
import type { ForkJoinContext, TriggeredSubworkflowContext } from "./context.js";
import type { ThreadVariable } from "./variables.js";
import type { NodeExecutionResult } from "./history.js";
import type { VariableScopes } from "./scopes.js";

/**
 * Workflow Execution Definition Type (Execution Instance)
 * WorkflowExecution as a pure data object, does not contain methods, which are provided by ThreadContext
 *
 * Note: Runtime state fields (status, startTime, endTime, shouldPause, shouldStop)
 * are managed by ThreadState in ThreadEntity, not stored in this data object.
 */
export interface WorkflowExecution {
  /** Thread Unique Identifier */
  id: ID;
  /** Associated Workflow ID */
  workflowId: ID;
  /** Workflow version */
  workflowVersion: Version;
  /** Current execution node ID */
  currentNodeId: ID;
  /** Preprocessed workflow graph structure (using the WorkflowGraph interface) */
  graph: WorkflowGraph;
  /** Array of variables (for persistence and metadata) */
  variables: ThreadVariable[];
  /** Four levels of scope variable storage */
  variableScopes: VariableScopes;
  /**
   * Input data (as a special variable, accessible via path)
   *
   * Description: Stores the input data for the workflow
   * - Initialized when the START node is executed
   * - Can be accessed through expression parsing (using input.)
   * - Remains unchanged throughout the workflow execution
   * - Used to pass external inputs into the workflow
   *
   * Example:
   * ```typescript
   * thread.input = {
   *   userName: 'Alice',
   *   userAge: 25,
   *   config: { timeout: 5000 }
   * }
   *
   * // Access in expressions
   * {{input.userName}}  // 'Alice'
   * {{input.config.timeout}}  // 5000
   * ```
   *
   * Note: Difference between this field and variables
   * - Thread.input: Initial input of the workflow, read-only
   * - Thread.variableScopes.thread: Variables used during workflow execution, mutable
   */
  input: Record<string, unknown>;
  /**
   * Output data (as a special variable, accessible via path)
   *
   * Description: Stores the final output data of the workflow
   * - Set when the END node is executed
   * - Can be accessed through expression parsing (using output.)
   * - Defaults to an empty object, populated by the END node or the last node
   * - Used to return the execution result of the workflow
   *
   * Example:
   * ```typescript
   * thread.output = {
   *   result: 'Task completed',
   *   status: 'success',
   *   data: { count: 10 }
   * }
   *
   * // Access in expressions
   * {{output.result}}  // 'Task completed'
   * {{output.data.count}}  // 10
   * ```
   *
   * Note: Difference between this field and variables
   * - Thread.output: Final output of the workflow, read-only
   * - Thread.variableScopes.thread: Variables used during workflow execution, mutable
   */
  output: Record<string, unknown>;
  /** Execution history (stored in order of execution) */
  nodeResults: NodeExecutionResult[];
  /** Error Message Array */
  errors: unknown[];
  /** Context data (for storing instances of Conversation, etc.) */
  contextData?: Record<string, unknown>;

  // ========== Thread types and relationship management ==========
  /** Thread type */
  threadType?: WorkflowExecutionType;

  /** FORK/JOIN context (only present if threadType is FORK_JOIN) */
  forkJoinContext?: ForkJoinContext;

  /** Triggered subworkflow context (only present if threadType is TRIGGERED_SUBWORKFLOW) */
  triggeredSubworkflowContext?: TriggeredSubworkflowContext;
}
