/**
 * Tool Approval Types
 * Types for tool approval coordination
 */

import type { LLMToolCall } from "../message/index.js";
import type { AutoApprovalCategory } from "./risk-level.js";
import type { FilePermissionSettings } from "./file-permission.js";
import type { McpApprovalSettings, McpRequest } from "./mcp-approval.js";
import type { ToolExecutionResult } from "./execution.js";

/**
 * Security Preset
 * Predefined security configurations for tool approval
 */
export type SecurityPreset = "SAFE" | "BALANCED" | "PERMISSIVE";

/**
 * Workspace boundary control settings
 */
export interface WorkspaceBoundarySettings {
  /** Allow read operations outside workspace */
  allowReadOnlyOutsideWorkspace?: boolean;
  /** Allow write operations outside workspace */
  allowWriteOutsideWorkspace?: boolean;
}

/**
 * Command execution settings
 */
export interface CommandExecutionSettings {
  /** Allowed command prefixes (longest prefix match) */
  allowedCommands?: string[];
  /** Denied command prefixes (longest prefix match) */
  deniedCommands?: string[];
}

/**
 * Network request settings (for HTTP tools like web_fetch)
 */
export interface NetworkSettings {
  /** Allowed domains */
  allowedDomains?: string[];
  /** Denied domains */
  deniedDomains?: string[];
}

/**
 * Interaction settings
 */
export interface InteractionSettings {
  /** Followup question timeout (ms) for auto-selecting suggestion */
  followupAutoApproveTimeoutMs?: number;
}

/**
 * Tool Approval Options
 * Options for tool approval coordinator
 * Note: This is different from ToolApprovalConfig in workflow/config.ts
 * which is used for workflow-level configuration
 */
export interface ToolApprovalOptions {
  // === Main switch ===
  /** Enable auto-approval system (default: false) */
  autoApprovalEnabled?: boolean;

  // === Presets ===
  /** Security preset to apply (overrides manual category settings if provided) */
  securityPreset?: SecurityPreset;

  // === File permission (HIGHEST PRIORITY) ===
  /** File permission settings - evaluated first before any other checks */
  filePermissions?: FilePermissionSettings;

  // === Category-based settings ===
  /** Category-based auto-approval settings */
  categories?: Partial<Record<AutoApprovalCategory, boolean>>;

  // === Boundary settings ===
  /** Workspace boundary controls */
  workspaceBoundary?: WorkspaceBoundarySettings;
  /** Allow writing to protected files */
  allowWriteProtected?: boolean;

  // === Operation-specific settings ===
  /** Command execution settings */
  command?: CommandExecutionSettings;
  /** Network request settings (for HTTP tools) */
  network?: NetworkSettings;
  /** MCP approval settings */
  mcp?: McpApprovalSettings;
  /** Interaction settings */
  interaction?: InteractionSettings;

  // === Usage Limits ===
  /** Maximum number of consecutive auto-approved requests before requiring manual approval */
  maxAutoApprovedRequests?: number;

  // === General settings ===
  /** Approval timeout in milliseconds (0 = no timeout) */
  approvalTimeout?: number;
}

/**
 * Tool Approval Request
 * Request for tool approval
 */
export interface ToolApprovalRequest {
  /** Tool call to approve */
  toolCall: LLMToolCall;
  /** Tool description */
  toolDescription?: string;
  /** Context ID (execution ID, session ID, etc.) */
  contextId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Interaction ID for tracking */
  interactionId: string;
  
  // Batch execution metadata
  /** Unique ID for this batch */
  batchId?: string;
  /** Position in batch (0-based) */
  toolIndex?: number;
  /** Total tools in batch */
  totalTools?: number;
  /** Tools remaining after this one */
  pendingQueue?: LLMToolCall[];
  /** Results from auto-executed prefix */
  autoExecutedResults?: ToolExecutionResult[];
}

/**
 * Tool Approval Result
 * Result of tool approval process
 */
export interface ToolApprovalResult {
  /** Whether the tool call is approved */
  approved: boolean;
  /** Tool call ID */
  toolCallId: string;
  /** Edited parameters (if user modified them) */
  editedParameters?: Record<string, unknown>;
  /** User instruction (optional additional context) */
  userInstruction?: string;
  /** User's comment explaining decision */
  annotation?: string;
  /** Rejection reason (if not approved) */
  rejectionReason?: string;
  
  // Batch control
  /** Should continue with remaining tools? */
  continueBatch?: boolean;
}

/**
 * Tool Approval Handler
 * Interface for handling tool approval requests
 */
export interface ToolApprovalHandler {
  /**
   * Request tool approval
   * @param request Approval request
   * @returns Approval result
   */
  requestApproval(request: ToolApprovalRequest): Promise<ToolApprovalResult>;
}

/**
 * Tool Batch Result
 * Result of processing a batch of tools
 */
export interface ToolBatchResult {
  /** Unique ID for this batch */
  batchId: string;
  /** Auto-executed tools */
  autoExecuted: ToolExecutionResult[];
  /** First tool needing approval */
  confirmationRequired: LLMToolCall | null;
  /** User's decision */
  confirmationResult?: ToolApprovalResult;
  /** Tools not yet processed */
  remainingQueue: LLMToolCall[];
  /** True if all tools done */
  allCompleted: boolean;
}

/**
 * Pending Tool Call
 * Information about a pending tool call for UI display
 */
export interface PendingToolCall {
  /** Tool call ID */
  id: string;
  /** Tool name */
  name: string;
  /** Tool arguments */
  arguments?: string;
  /** Risk level */
  riskLevel?: import("./risk-level.js").ToolRiskLevel;
}

/**
 * Tool Approval Coordinator Params
 * Parameters for tool approval coordinator
 */
export interface ToolApprovalCoordinatorParams {
  /** Tool call to process */
  toolCall: LLMToolCall;
  /** Approval options */
  options?: ToolApprovalOptions;
  /** Context ID */
  contextId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Tool description (optional) */
  toolDescription?: string;
  /** Approval handler */
  approvalHandler: ToolApprovalHandler;
}
