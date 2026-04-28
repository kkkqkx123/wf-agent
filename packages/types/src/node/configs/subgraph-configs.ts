/**
 * Subgraph node configuration type definition
 */

import type { ID } from '../../common.js';
import type { MessageRole } from '../../message/index.js';
import type { TruncateMessageOperation, FilterMessageOperation } from '../../message/index.js';

/**
 * Subgraph node configuration
 */
export interface SubgraphNodeConfig {
  /** Subworkflow ID */
  subgraphId: ID;
  /** Whether to execute asynchronously */
  async: boolean;
}

/**
 * Configuration of nodes starting from triggers
 * Used exclusively to identify the start point of an isolated sub-workflow initiated by a trigger
 * Empty configuration, used only as an identifier
 */
export type StartFromTriggerNodeConfig = object;

/**
 * Node Configuration Continued from Trigger (Batch Aware)
 * Used to call back data to the master workflow after the execution of the sub workflow is complete
 */
export interface ContinueFromTriggerNodeConfig {
  /** Variable Callback Configuration */
  variableCallback?: {
    /** List of variable names to return */
    includeVariables?: string[];
    /** Whether to pass back all variables (default false) */
    includeAll?: boolean;
  };
  /** Dialog history callback configuration (batch aware) */
  conversationHistoryCallback?: {
    /** Type of operation */
    operation: 'TRUNCATE' | 'FILTER';

    /** Truncate Operation Configuration */
    truncate?: TruncateMessageOperation & {
      /** Return the last N visible messages */
      lastN?: number;
      /** Returns the last N visible messages for the specified role */
      lastNByRole?: {
        role: MessageRole;
        count: number;
      };
    };

    /** Filter Operation Configuration */
    filter?: FilterMessageOperation & {
      /** Returns all visible messages for the specified role */
      byRole?: MessageRole;
      /** Returns a specified range of visible messages */
      range?: {
        start: number;
        end: number;
      };
    };
  };
  /** Callback Options */
  callbackOptions?: {
    /** Whether only visible messages are returned */
    visibleOnly?: boolean;
  };
}