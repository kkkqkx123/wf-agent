/**
 * Auto Approval Checker
 * Core logic for determining if a tool call should be auto-approved
 */

import type {
  Tool,
  ToolApprovalOptions,
  ToolRiskLevel,
  McpRequest,
  FileOperationType,
} from "@wf-agent/types";
import { getApprovalCategory, canAutoApprove } from "@wf-agent/types";
import { checkFilePermission } from "./file-permission-checker.js";
import { getCommandDecision } from "./command-safety-checker.js";
import { checkMcpApproval } from "./mcp-approval-checker.js";

/**
 * Auto-approval decision types
 */
export type AutoApprovalDecision =
  | { decision: "approve" }
  | { decision: "deny"; reason: string }
  | { decision: "ask" }
  | { decision: "timeout"; timeout: number; autoResponse: unknown };

/**
 * Context for auto-approval check
 */
export interface AutoApprovalContext {
  /** Whether operation is outside workspace */
  isOutsideWorkspace?: boolean;
  /** Whether target is a protected file */
  isProtected?: boolean;
  /** File path (for file operations) */
  filePath?: string;
  /** File operation type */
  fileOperation?: FileOperationType;
  /** Command string (for EXECUTE tools) */
  command?: string;
  /** Domain (for NETWORK tools) */
  domain?: string;
  /** MCP request (for MCP tools) */
  mcpRequest?: McpRequest;
  /** Followup suggestion (for INTERACTION tools) */
  followupSuggestion?: string;
}

/**
 * Parameters for auto-approval check
 */
export interface CheckAutoApprovalParams {
  /** Approval options */
  options: ToolApprovalOptions;
  /** Tool being checked */
  tool: Tool;
  /** Execution context */
  context: AutoApprovalContext;
}

/**
 * Check if a tool call should be auto-approved
 *
 * Priority order:
 * 1. File permission check (highest priority)
 * 2. Tool risk level check
 * 3. Category-based approval
 * 4. Operation-specific checks
 * 5. User approval (fallback)
 *
 * @param params - Check parameters
 * @returns Approval decision
 */
export function checkAutoApproval(params: CheckAutoApprovalParams): AutoApprovalDecision {
  const { options, tool, context } = params;

  // 0. Check if auto-approval is enabled
  if (!options.autoApprovalEnabled) {
    return { decision: "ask" };
  }

  // 1. File permission check (HIGHEST PRIORITY)
  if (options.filePermissions && context.filePath) {
    const fileResult = checkFilePermission(
      context.filePath,
      context.fileOperation ?? "read",
      options.filePermissions,
    );

    if (!fileResult.allowed) {
      return { decision: "deny", reason: fileResult.reason ?? "File permission denied" };
    }
  }

  // 2. Get risk level
  const riskLevel = tool.metadata?.riskLevel ?? "WRITE";

  // 3. SYSTEM level never auto-approves
  if (riskLevel === "SYSTEM") {
    return { decision: "ask" };
  }

  // 4. Check tool-specific autoApprovable flag
  if (tool.metadata?.autoApprovable === false) {
    return { decision: "ask" };
  }
  if (tool.metadata?.autoApprovable === true) {
    return { decision: "approve" };
  }

  // 5. Check legacy autoApprovedTools list
  if (options.autoApprovedTools?.includes(tool.id)) {
    return { decision: "approve" };
  }

  // 6. Handle by risk level
  switch (riskLevel) {
    case "READ_ONLY":
      return handleReadOnlyApproval(options, context);

    case "WRITE":
      return handleWriteApproval(options, context);

    case "EXECUTE":
      return handleExecuteApproval(options, context);

    case "MCP":
      return handleMcpApproval(options, context);

    case "NETWORK":
      return handleNetworkApproval(options, context);

    case "INTERACTION":
      return handleInteractionApproval(options, context);

    default:
      return { decision: "ask" };
  }
}

/**
 * Handle READ_ONLY approval
 */
function handleReadOnlyApproval(
  options: ToolApprovalOptions,
  context: AutoApprovalContext,
): AutoApprovalDecision {
  // Check category setting
  if (!options.categories?.alwaysAllowReadOnly) {
    return { decision: "ask" };
  }

  // Check workspace boundary
  if (context.isOutsideWorkspace && !options.workspaceBoundary?.allowReadOnlyOutsideWorkspace) {
    return { decision: "ask" };
  }

  return { decision: "approve" };
}

/**
 * Handle WRITE approval
 */
function handleWriteApproval(
  options: ToolApprovalOptions,
  context: AutoApprovalContext,
): AutoApprovalDecision {
  // Check category setting
  if (!options.categories?.alwaysAllowWrite) {
    return { decision: "ask" };
  }

  // Check workspace boundary
  if (context.isOutsideWorkspace && !options.workspaceBoundary?.allowWriteOutsideWorkspace) {
    return { decision: "ask" };
  }

  // Check protected file
  if (context.isProtected && !options.allowWriteProtected) {
    return { decision: "ask" };
  }

  return { decision: "approve" };
}

/**
 * Handle EXECUTE approval
 */
function handleExecuteApproval(
  options: ToolApprovalOptions,
  context: AutoApprovalContext,
): AutoApprovalDecision {
  // Check category setting
  if (!options.categories?.alwaysAllowExecute) {
    return { decision: "ask" };
  }

  const command = context.command;
  if (!command) {
    return { decision: "ask" };
  }

  // Check command whitelist/denylist
  const decision = getCommandDecision(
    command,
    options.command?.allowedCommands ?? [],
    options.command?.deniedCommands,
  );

  switch (decision) {
    case "auto_approve":
      return { decision: "approve" };
    case "auto_deny":
      return { decision: "deny", reason: "Command is in denylist or not in allowlist" };
    default:
      return { decision: "ask" };
  }
}

/**
 * Handle MCP approval (independent category)
 */
function handleMcpApproval(
  options: ToolApprovalOptions,
  context: AutoApprovalContext,
): AutoApprovalDecision {
  // Check category setting
  if (!options.categories?.alwaysAllowMcp) {
    return { decision: "ask" };
  }

  // If MCP settings are provided, use fine-grained control
  if (options.mcp && context.mcpRequest) {
    const mcpDecision = checkMcpApproval({
      settings: options.mcp,
      request: context.mcpRequest,
    });

    switch (mcpDecision.decision) {
      case "approve":
        return { decision: "approve" };
      case "deny":
        return { decision: "deny", reason: mcpDecision.reason };
      case "ask":
        return { decision: "ask" };
    }
  }

  return { decision: "approve" };
}

/**
 * Handle NETWORK approval
 */
function handleNetworkApproval(
  options: ToolApprovalOptions,
  context: AutoApprovalContext,
): AutoApprovalDecision {
  // Check category setting
  if (!options.categories?.alwaysAllowNetwork) {
    return { decision: "ask" };
  }

  const domain = context.domain;
  if (!domain) {
    return { decision: "ask" };
  }

  // Check domain whitelist/denylist
  const allowedDomains = options.network?.allowedDomains ?? [];
  const deniedDomains = options.network?.deniedDomains ?? [];

  // Check denylist first
  if (deniedDomains.some(d => domain.includes(d))) {
    return { decision: "deny", reason: `Domain ${domain} is in denylist` };
  }

  // Check allowlist
  if (allowedDomains.length > 0 && !allowedDomains.some(d => domain.includes(d))) {
    return { decision: "ask" };
  }

  return { decision: "approve" };
}

/**
 * Handle INTERACTION approval (followup question)
 */
function handleInteractionApproval(
  options: ToolApprovalOptions,
  context: AutoApprovalContext,
): AutoApprovalDecision {
  // Check category setting
  if (!options.categories?.alwaysAllowInteraction) {
    return { decision: "ask" };
  }

  const timeout = options.interaction?.followupAutoApproveTimeoutMs;

  if (timeout && timeout > 0 && context.followupSuggestion) {
    return {
      decision: "timeout",
      timeout,
      autoResponse: context.followupSuggestion,
    };
  }

  return { decision: "ask" };
}

/**
 * Extract context from tool call parameters
 *
 * @param toolId - Tool ID
 * @param parameters - Tool call parameters
 * @returns Extracted context
 */
export function extractContextFromParameters(
  toolId: string,
  parameters: Record<string, unknown>,
): AutoApprovalContext {
  const context: AutoApprovalContext = {};

  // Extract file path for file tools
  if (["read_file", "write_file", "edit", "apply_diff", "apply_patch"].includes(toolId)) {
    context.filePath = parameters["path"] as string;
    context.fileOperation = toolId === "read_file" ? "read" : "write";
  }

  // Extract command for shell tools
  if (["run_shell", "backend_shell", "run_slash_command"].includes(toolId)) {
    context.command = parameters["command"] as string;
  }

  // Extract MCP request for MCP tools
  if (toolId === "use_mcp") {
    context.mcpRequest = {
      type: "use_mcp",
      serverName: parameters["server_name"] as string,
      toolName: parameters["tool_name"] as string,
      arguments: parameters["arguments"] as Record<string, unknown>,
    };
  }

  return context;
}
