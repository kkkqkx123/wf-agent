/**
 * Sync Node Configuration Type Definition
 * 
 * Purpose:
 * - Explicit synchronization between fork branches
 * - Variable and message passing from source branch to target branch
 * - Eliminates implicit state sharing through global variables
 */

import type { ID } from '../../common.js';
import type { WorkflowVariableInput } from '../../workflow/boundary-config.js';

/**
 * Sync Node Output
 * - syncedFromPath: ID - The source path ID from which data was synced
 * - syncedVariables?: Record<string, unknown> - The variables transferred from source to target
 * - completed: boolean - Whether the sync operation completed successfully
 */
export interface SyncNodeOutput {
  syncedFromPath: ID;
  syncedVariables?: Record<string, unknown>;
  completed: boolean;
}

/**
 * Sync Node Configuration
 *
 * Description:
 * - SYNC nodes provide explicit data transfer between parallel execution branches
 * - Variables are deep cloned during transfer to maintain complete isolation
 * - Can optionally wait for source branch completion before syncing
 * - Supports both variable and message context synchronization
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
   */
  variableMappings?: WorkflowVariableInput[];
  
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
}
