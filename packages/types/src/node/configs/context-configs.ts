/**
 * Context Processor Node Configuration Type Definition (Batch Aware)
 * Used to directly manipulate the prompt word message array, supporting truncation, insertion, replacement, filtering, clearing, etc.
 */

import type { MessageOperationConfig } from '../../message/index.js';

/**
 * Context processor node configuration (batch-aware)
 * Used to directly manipulate the prompt word message array, supporting truncation, insertion, replacement, filtering, clearing and other operations
 */
export interface ContextProcessorNodeConfig {
  /** Configuration version (optional, default 4) */
  version?: number;
  /** Message operation configuration (batch-aware) */
  operationConfig: MessageOperationConfig;
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