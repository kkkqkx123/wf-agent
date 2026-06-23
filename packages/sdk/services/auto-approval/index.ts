/**
 * Auto Approval Service
 * Unified export for auto-approval functionality
 */

// Core checker
export {
  checkAutoApproval,
  extractContextFromParameters,
  type AutoApprovalDecision,
  type AutoApprovalContext,
  type CheckAutoApprovalParams,
} from "./auto-approval-checker.js";

// Command safety (re-exported from shared module for convenience)
export {
  containsDangerousSubstitution,
  findLongestPrefixMatch,
  getCommandDecision,
  getSingleCommandDecision,
  type CommandDecision,
} from "../command-safety/command-safety-checker.js";

// File permission
export {
  checkFilePermission,
  matchesPattern,
  getEffectivePermission,
  batchCheckFilePermissions,
  createDefaultFilePermissionSettings,
} from "./file-permission-checker.js";

// MCP approval
export {
  checkMcpApproval,
  createDefaultMcpApprovalSettings,
  mergeMcpApprovalSettings,
  isServerConfigured,
  getAutoApprovedTools,
} from "./mcp-approval-checker.js";
