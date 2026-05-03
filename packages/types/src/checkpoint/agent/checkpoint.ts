/**
 * Agent Loop Checkpoint Type Definition
 */

import type { ID, Timestamp } from "../../common.js";
import type { Message } from "../../message/index.js";
import type { IterationRecord } from "../../agent-execution/types.js";
import { AgentLoopStatus } from "../../agent-execution/types.js";
import type { AnyCheckpoint } from "../base.js";
import type { AgentLoopStateSnapshot } from "./snapshot.js";

/**
 * Agent Loop Incremental Data Structure
 */
export interface AgentLoopDelta {
  /** New messages */
  addedMessages?: Message[];

  /** New iteration record */
  addedIterations?: IterationRecord[];

  /** Modified variable */
  modifiedVariables?: Map<string, unknown>;

  /** Status change */
  statusChange?: {
    from: AgentLoopStatus;
    to: AgentLoopStatus;
  };

  /** Other status differences */
  otherChanges?: Record<string, unknown>;
}

/**
 * Agent Loop Checkpoint
 * Uses AnyCheckpoint union type for strong typing
 */
export type AgentLoopCheckpoint = AnyCheckpoint<AgentLoopDelta, AgentLoopStateSnapshot> & {
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
};
