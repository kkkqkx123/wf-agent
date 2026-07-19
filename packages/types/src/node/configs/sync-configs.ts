/**
 * Sync Node Configuration Type Definition
 * 
 * Purpose:
 * - Explicit synchronization between fork branches
 * - Variable, data, and message passing from source branch to target branch
 * - Eliminates implicit state sharing through global variables
 * - Paired SYNC for bidirectional data exchange between main and sub-workflows
 */

import type { ID } from '../../common.js';
import type { WorkflowVariableInput, WorkflowDataInput, WorkflowMessageInput } from '../../workflow/boundary-config.js';

/**
 * Sync Node Output
 * - syncedFromPath: ID - The source path ID from which data was synced
 * - syncedVariables?: Record<string, unknown> - The variables transferred from source to target
 * - syncedVariableCount?: number - Number of variables synced
 * - syncedDataCount?: number - Number of data inputs processed
 * - syncedMessageCount?: number - Number of message contexts synced
 * - completed: boolean - Whether the sync operation completed successfully
 */
export interface SyncNodeOutput {
  syncedFromPath: ID;
  syncedVariables?: Record<string, unknown>;
  syncedVariableCount?: number;
  syncedDataCount?: number;
  syncedMessageCount?: number;
  completed: boolean;
}

/**
 * Variable exchange entry for paired SYNC
 *
 * Defines a bidirectional variable exchange between a source and target path.
 * Used in conjunction with pairId to enable data synchronization between
 * main workflow and sub-workflow SYNC nodes.
 */
export interface SyncVariableExchange {
  /** Source path ID within the current workflow's FORK-JOIN structure */
  sourcePathId: ID;
  /** Source variable name in the source path context */
  sourceVariable: string;
  /** Target path ID within the current workflow's FORK-JOIN structure */
  targetPathId: ID;
  /** Target variable name in the target path context */
  targetVariable: string;
}

/**
 * Sync Node Configuration
 *
 * Description:
 * - SYNC nodes provide explicit data transfer between parallel execution branches
 * - Variables are deep cloned during transfer to maintain complete isolation
 * - Can optionally wait for source branch completion before syncing
 * - Supports variable, data input, and message context synchronization
 * - Paired SYNC (with pairId) enables bidirectional data exchange between
 *   main and sub-workflows for coordinated synchronization
 */
export interface SyncNodeConfig {
  /**
   * Source path ID - the branch to sync from
   * Must be one of the forkPathIds in the parent FORK node
   */
  sourcePathId: ID;
  
  /**
   * Target path ID - the current branch (optional, auto-detected if not provided)
   * Must be one of the forkPathIds in the parent FORK node
   */
  targetPathId?: ID;
  
  /**
   * Variable mappings - explicit variable transfer configuration
   * Maps variables from source branch to target branch with deep cloning
   *
   * Uses WorkflowVariableInput for consistency with other boundary configurations.
   * sourcePath resolves the value from the source branch,
   * internalName is the target branch variable name.
   */
  variableMappings?: WorkflowVariableInput[];
  
  /**
   * Data inputs - maps fields from the execution input to internal variables
   * in the target branch.
   *
   * Unlike variableMappings (which maps source branch variables to target),
   * dataInputs maps the parent execution input data directly to the target
   * branch's variables. This ensures the target branch has access to the
   * original execution input fields after synchronization.
   *
   * Example:
   *   Execution input: { userId: "abc", query: "hello" }
   *   dataInputs: [{ parentField: "query", internalName: "query_text" }]
   *   Result: target branch variable "query_text" gets "hello"
   */
  dataInputs?: WorkflowDataInput[];
  
  /**
   * Message context inputs - sync message contexts from source branch
   * to target branch using the boundary-config pattern.
   *
   * Maps named message contexts from the source branch's registry
   * to the target branch's registry.
   *
   * Note: Message context synchronization uses shallow copying
   * (reference-level isolation) to prevent unintended mutations.
   */
  messageInputs?: WorkflowMessageInput[];
  
  /**
   * Whether to wait for source branch completion before syncing
   * If true, the SYNC node will block until the source branch completes
   * If false, sync happens immediately (may get stale/incomplete data)
   * Default: true
   */
  waitForCompletion?: boolean;
  
  /**
   * Timeout in seconds when waitForCompletion is true
   * 0 means no timeout (wait indefinitely)
   * > 0 means maximum wait time in seconds
   * Default: 0 (no timeout)
   */
  timeout?: number;

  /**
   * Pair ID - identifies paired SYNC nodes across main and sub-workflow graphs.
   *
   * When set, this SYNC node participates in a paired synchronization:
   * - The same pairId must appear exactly once in the main workflow
   *   and once in the sub-workflow
   * - Enables bidirectional data exchange between the two workflows
   * - Format convention: `${mainWorkflowId}:${subWorkflowId}:<name>`
   *
   * When not set, the SYNC node follows the existing FORK-JOIN branch
   * synchronization logic (backward compatible).
   */
  pairId?: string;

  /**
   * Variable exchanges - bidirectional variable exchange configuration
   * for paired SYNC nodes.
   *
   * Each entry defines a variable exchange between a source path and
   * a target path within the respective workflow's FORK-JOIN structure.
   *
   * When paired SYNC is used (pairId is set), variableExchanges provide
   * a flexible multi-source multi-target data exchange mechanism that
   * complements the existing unidirectional variableMappings.
   */
  variableExchanges?: SyncVariableExchange[];
}
