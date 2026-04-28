/**
 * Agent Loop Checkpoint Type Definition
 */

import type { ID, Timestamp } from "../../common.js";
import type { Message } from "../../message/index.js";
import type { IterationRecord } from "../../agent/records.js";
import { AgentLoopStatus } from "../../agent/status.js";
import type { BaseCheckpoint } from "../base.js";
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
  otherChanges?: Record<string, { from: unknown; to: unknown }>;
}

/**
 * Agent Loop Checkpoint
 * Extends BaseCheckpoint with Agent-specific fields
 */
export interface AgentLoopCheckpoint
  extends BaseCheckpoint<AgentLoopDelta, AgentLoopStateSnapshot> {
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
}
