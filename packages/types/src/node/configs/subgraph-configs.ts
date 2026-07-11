/**
 * Subgraph Node Configuration Type Definition
 * 
 * Configuration for SUBGRAPH nodes that embed workflows as black-box components
 * within the graph structure. These nodes are expanded during preprocessing.
 * 
 * Design Philosophy:
 * - Explicit variable mapping (like function parameters)
 * - Variable isolation through scope stack mechanism
 * - Clear interface contract between parent and child workflows
 * - Reuses WorkflowBoundaryConfig for consistent boundary handling across all workflow types
 */

import type { ID } from '../../common.js';
import type { WorkflowVariableInput, WorkflowVariableOutput, WorkflowDataInput, WorkflowMessageInput, WorkflowMessageOutput } from '../../workflow/boundary-config.js';

/**
 * Subgraph Node Output
 * - executionResult: { output?: unknown, status: string } - The sub-workflow execution result
 * - duration: number - Execution duration in milliseconds
 */
export interface SubgraphNodeOutput {
  executionResult: {
    output?: unknown;
    status: string;
  };
  duration: number;
}

/**
 * Subgraph node configuration
 *
 * Used for embedding workflows as black-box nodes within the graph structure.
 * The subgraph is expanded and merged into the parent graph during preprocessing.
 *
 * IMPORTANT: All variable passing must be explicit through variableInputs.
 * There is NO automatic scope inheritance - subgraphs cannot access parent variables
 * unless explicitly mapped. Variable isolation is achieved through VariableManager's
 * scope stack mechanism.
 *
 * Note: This configuration reuses WorkflowVariableInput/WorkflowVariableOutput from
 * boundary-config.ts to maintain consistency with other workflow boundary configurations
 * (START, END, START_FROM_TRIGGER, CONTINUE_FROM_TRIGGER).
 */
export interface SubgraphNodeConfig {
  /** Subworkflow ID to embed */
  subgraphId: ID;
  
  /** Whether to execute asynchronously */
  async: boolean;

  /**
   * Failure handling strategy for subgraph execution.
   * - 'fail': Propagate the error to the parent workflow (default).
   * - 'continue': Return a SKIPPED result with optional fallbackOutput, allowing the parent to continue.
   * - 'retry': Retry the subgraph execution up to maxRetries times before failing.
   */
  onFailure?: 'fail' | 'continue' | 'retry';

  /** Maximum number of retry attempts (only used when onFailure is 'retry'). Default: 3 */
  maxRetries?: number;

  /** Base delay between retries in milliseconds (applies exponential backoff). Default: 1000 */
  retryDelayMs?: number;

  /**
   * Fallback output value when continuing on failure.
   * Only used when onFailure is 'continue'.
   */
  fallbackOutput?: Record<string, unknown>;
  
  /**
   * Variable Input Mapping
   *
   * Explicitly defines which parent workflow variables are passed to the subgraph.
   * This is the ONLY way for a subgraph to receive data from its parent.
   *
   * During graph preprocessing, these mappings are transferred to the subgraph's
   * START node configuration, making them available at runtime.
   *
   * Uses WorkflowVariableInput for consistency with other boundary configurations.
   *
   * Example:
   * ```typescript
   * variableInputs: [
   *   {
   *     externalName: "apiKey",        // Parent's variable
   *     internalName: "api_key",       // Child sees it as this name
   *     required: true,
   *     description: "API key for authentication"
   *   },
   *   {
   *     externalName: "config",
   *     internalName: "settings",
   *     defaultValue: { timeout: 5000 }
   *   }
   * ]
   * ```
   */
  variableInputs?: WorkflowVariableInput[];
  
  /**
   * Variable Output Mapping
   *
   * Defines how subgraph variables are returned to the parent workflow.
   * Uses WorkflowVariableOutput for consistency with other boundary configurations.
   *
   * Note: While this field exists in the SUBGRAPH node config for documentation
   * and validation purposes, actual output handling is done through the subgraph's
   * END node configuration or execution result.
   */
  variableOutputs?: WorkflowVariableOutput[];

  /**
   * Data Input Mapping
   *
   * Explicitly maps fields from the parent workflow's execution input data
   * to the subgraph's internal variables.
   *
   * Unlike variableInputs (which maps parent variables to child variables),
   * dataInputs maps the raw execution input data (WorkflowExecution.input)
   * to child variables. This enables passing top-level execution data across
   * the subgraph boundary without relying on implicit inheritance.
   *
   * Designed to work with the parent workflow's WorkflowStartConfig.dataInputs
   * for end-to-end explicit data passing.
   *
   * Example:
   * Parent workflow execution input: { userId: "abc", query: "hello" }
   * Subgraph dataInputs:
   *   { parentField: "userId", internalName: "user_id", required: true }
   *   { parentField: "query", internalName: "query_text" }
   * Result: child variables user_id = "abc", query_text = "hello"
   *
   * IMPORTANT: This is the ONLY way for a subgraph to access parent execution
   * input data. There is NO automatic data inheritance.
   */
  dataInputs?: WorkflowDataInput[];

  /**
   * Message context passing configuration
   *
   * Maps parent workflow contexts to subgraph inputs,
   * and subgraph outputs back to parent workflow contexts.
   * This provides explicit control over message context flow between workflows.
   *
   * Reuses WorkflowMessageInput/WorkflowMessageOutput from boundary-config
   * for consistency with other workflow boundary configurations.
   *
   * For inputs: externalName = parent context registry key, internalName = subgraph internal name
   * For outputs: internalName = subgraph internal name, externalName = parent context registry key
   */
  messagePassing?: {
    /** Input mappings - parent registry key → subgraph internal name */
    inputs?: WorkflowMessageInput[];
    
    /** Output mappings - subgraph internal name → parent registry key */
    outputs?: WorkflowMessageOutput[];
  };
}