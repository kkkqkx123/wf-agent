/**
 * Workflow Execution state snapshot type definition
 */

import type { ID } from "../../common.js";
import { WorkflowExecutionStatus } from "../../workflow-execution/index.js";
import type { NodeExecutionResult, VariableScopes, TriggeredSubworkflowContext } from "../../workflow-execution/index.js";
import type { TriggerRuntimeState } from "../../trigger/index.js";
import type { TokenUsageStats } from "../../llm/index.js";
import type { MessageMarkMap } from "../../message/index.js";

/**
 * Workflow Execution state snapshot type
 */
export interface WorkflowExecutionStateSnapshot {
  /** Execution Status */
  status: WorkflowExecutionStatus;
  /** Current node ID */
  currentNodeId: ID;
  /** Variable array */
  variables: unknown[];
  /** Variable Scope Snapshot (used for restoring runtime state) */
  variableScopes: VariableScopes;
  /** Input data */
  input: Record<string, unknown>;
  /** Output data */
  output: Record<string, unknown>;
  /** Node execution result mapping */
  nodeResults: Record<string, NodeExecutionResult>;
  /** Error message array */
  errors: unknown[];
  /** Conversation state (stores the complete message history and index information, used for restoring the ConversationSession) */
  conversationState: {
    /** Full message history array */
    messages: unknown[];
    /** Message Tag Mapping */
    markMap: MessageMarkMap;
    /** Token Usage Statistics */
    tokenUsage: TokenUsageStats | null;
    /** The current request token is in use. */
    currentRequestUsage: TokenUsageStats | null;
  };
  /** Tool Approval Status (used to resume tool calls that are waiting for approval) */
  toolApprovalState?: {
    /** The tool call that is currently awaiting approval. */
    pendingToolCall?: {
      /** Tool call ID */
      id: string;
      /** Tool Name */
      name: string;
      /** Tool parameters */
      arguments: string;
    };
    /** Interaction ID */
    interactionId: string;
    /** Approval timeout period */
    timeout: number;
  };
  /** Trigger state snapshot (used for restoring TriggerState) */
  triggerStates?: Map<ID, TriggerRuntimeState>;
  /** FORK/JOIN context (master-slave separation mode) */
  forkJoinContext?: {
    forkId: string;
    forkPathId: string;
  };
  /** Triggered Sub-workflow Context (Master-Slave Separation Mode) */
  triggeredSubworkflowContext?: TriggeredSubworkflowContext;
}
