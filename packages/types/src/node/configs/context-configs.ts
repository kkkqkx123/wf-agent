/**
 * Context Processor / Data Processor Node Configuration Type Definition (Batch Aware)
 *
 * Unified configuration for three data domain operations:
 * 1. Message operations - LLM conversation history management
 * 2. Variable operations - Workflow runtime variable aggregation and transformation
 * 3. Execution data operations - Input/output mapping (via MESSAGE/VARIABLE)
 */

import type { MessageOperationConfig } from '../../message/index.js';
import type { VariableOperationConfig, VariableOperationOutput } from './variable-operation-configs.js';

// Re-export VariableOperationOutput for convenient access
export type { VariableOperationOutput };

/**
 * Context Processor / Data Processor Node Output
 *
 * Result of any data processing operation:
 * - Message operations: message count and statistics
 * - Variable operations: modified variable list
 */
export type ContextProcessorNodeOutput =
  | MessageOperationOutput
  | VariableOperationOutput;

/**
 * Message operation output
 */
export interface MessageOperationOutput {
  operation: string;
  messageCount: number;
  sourceContext: string;
  targetContext: string;
  stats?: {
    originalMessageCount: number;
    visibleMessageCount: number;
    invisibleMessageCount: number;
  };
}

/**
 * Context processor / Data processor node configuration
 * Supports operations on three data domains: message, variable, and execution data
 */
export interface ContextProcessorNodeConfig {
  /** Configuration version (optional, default 4) */
  version?: number;

  /** Message operation configuration (batch-aware, optional) */
  operationConfig?: MessageOperationConfig;

  /** Variable operation configuration (optional) */
  variableOperation?: VariableOperationConfig;

  /**
   * Source context ID (for message operations)
   *
   * - If not specified: defaults to 'current'
   * - If specified: reads from the named context
   */
  sourceContext?: string;

  /**
   * Target context ID (for message operations)
   *
   * - If not specified: defaults to 'current'
   * - If specified: writes to the named context (auto-created if not exists)
   */
  targetContext?: string;

  /** Operational Options (for message operations) */
  operationOptions?: {
    /** Whether to manipulate only visible messages */
    visibleOnly?: boolean;
    /** Whether to automatically create new batches */
    autoCreateBatch?: boolean;
    /** Operation target: self (current execution, default) or parent (parent execution) */
    target?: 'self' | 'parent';
  };
}