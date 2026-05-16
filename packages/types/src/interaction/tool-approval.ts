/**
 * Tool Approval Types
 * Types related to tool approval requests and responses
 */

/**
 * Pending Tool Call Info
 * Information about a pending tool call for UI display
 */
export interface PendingToolCallInfo {
  /** Tool call ID */
  id: string;
  /** Tool name */
  name: string;
  /** Tool arguments */
  arguments?: string;
  /** Risk level */
  riskLevel?: import("../tool/risk-level.js").ToolRiskLevel;
}

/**
 * Structured Tool Approval Request Data
 * Rich context for tool approval requests
 */
export interface ToolApprovalRequestData {
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** Tool description */
  toolDescription?: string;
  /** Tool parameters */
  parameters: Record<string, unknown>;
  /** Risk level */
  riskLevel?: import("../tool/risk-level.js").ToolRiskLevel;
  /** Pending tools in queue */
  pendingQueue?: PendingToolCallInfo[];
  /** Auto-executed tools results */
  autoExecutedTools?: import("../tool/execution.js").ToolExecutionResult[];
  /** Batch ID */
  batchId?: string;
  /** Tool index in batch */
  toolIndex?: number;
  /** Total tools in batch */
  totalTools?: number;
  
  // Configuration fields (from ToolApprovalOptions)
  /** Approval timeout in milliseconds */
  timeout?: number;
  /** Security preset name */
  securityPreset?: import("../tool/approval.js").SecurityPreset;
}

/**
 * Structured Tool Approval Response Data
 * User's response to tool approval request
 */
export interface ToolApprovalResponseData {
  /** Whether approved */
  approved: boolean;
  /** Edited parameters */
  editedParameters?: Record<string, unknown>;
  /** User instruction */
  userInstruction?: string;
  /** User annotation/comment */
  annotation?: string;
  /** Rejection reason */
  rejectionReason?: string;
  /** Continue with remaining tools in batch */
  continueBatch?: boolean;
}
