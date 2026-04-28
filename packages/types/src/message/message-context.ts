/**
 * Message Operation Context Type Definition
 * Defines the context information required for a message operation
 */

import type { LLMMessage } from "./message.js";
import type { MessageMarkMap } from "./message-mark-map.js";

/**
 * Message Operation Context
 * Contains all the information needed to perform a message operation
 */
export interface MessageOperationContext {
  /** message array */
  messages: LLMMessage[];
  /** message tag mapping */
  markMap: MessageMarkMap;
  /** Operational Options */
  options?: {
    /** Whether to manipulate only visible messages */
    visibleOnly?: boolean;
    /** Whether to automatically create new batches */
    autoCreateBatch?: boolean;
  };
}
