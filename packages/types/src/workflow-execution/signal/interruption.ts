/**
 * Interrupt Request Type Definition
 * Define the types associated with workflow execution interrupt requests
 */

import type { ID } from "../../common.js";
import type { InterruptionType } from "../../errors/index.js";

/**
 * Interrupt Request Options
 */
export interface InterruptionRequestOptions {
  /** Interrupt Type */
  type: Exclude<InterruptionType, null>;
  /** Execution ID */
  executionId: ID;
  /** Node ID */
  nodeId: ID;
  /** Reason for interruption (optional) */
  reason?: string;
}
