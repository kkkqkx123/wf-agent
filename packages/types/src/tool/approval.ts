/**
 * Tool Approval Types
 * Types for tool approval coordination
 */

import type { LLMToolCall } from "../message/index.js";
import type { AutoApprovalCategory } from "./risk-level.js";
import type { FilePermissionSettings } from "./file-permission.js";
import type { McpApprovalSettings, McpRequest } from "./mcp-approval.js";

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

  // === Legacy support ===
  /** Legacy: List of auto-approved tool IDs/names */
  autoApprovedTools?: string[];

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
  /** Context ID (thread ID, session ID, etc.) */
  contextId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Interaction ID for tracking */
  interactionId: string;
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
  /** Rejection reason (if not approved) */
  rejectionReason?: string;
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
