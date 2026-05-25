/**
 * Context Processor Node Configuration Type Definition (Batch Aware)
 * Used to directly manipulate the prompt word message array, supporting truncation, insertion, replacement, filtering, clearing, etc.
 */

import type { MessageOperationConfig } from '../../message/index.js';

/**
 * Context Processor Node Output
 * - operation: string - The type of operation performed (e.g. TRUNCATE, APPEND, CLEAR, etc.)
 * - messageCount: number - Number of messages after processing
 * - sourceContext: string - The source context ID
 * - targetContext: string - The target context ID after processing
 * - stats: object - Optional statistics about the operation
 */
export interface ContextProcessorNodeOutput {
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
 * Context processor node configuration (batch-aware)
 * Used to directly manipulate the prompt word message array, supporting truncation, insertion, replacement, filtering, clearing and other operations
 */
export interface ContextProcessorNodeConfig {
  /** Configuration version (optional, default 4) */
  version?: number;
  
  /** Message operation configuration (batch-aware) */
  operationConfig: MessageOperationConfig;
  
  /**
   * Source context ID
   * 
   * - If not specified: defaults to 'current'
   * - If specified: reads from the named context
   */
  sourceContext?: string;
  
  /**
   * Target context ID
   * 
   * - If not specified: defaults to 'current'
   * - If specified: writes to the named context (auto-created if not exists)
   */
  targetContext?: string;
  
  /** Operational Options */
  operationOptions?: {
    /** Whether to manipulate only visible messages */
    visibleOnly?: boolean;
    /** Whether to automatically create new batches */
    autoCreateBatch?: boolean;
    /** Operation target: self (current execution, default) or parent (parent execution) */
    target?: 'self' | 'parent';
  };
}