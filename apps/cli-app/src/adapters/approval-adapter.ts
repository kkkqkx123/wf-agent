/**
 * Auto-Approval Configuration Adapter
 * Encapsulates auto-approval rule inspection and testing operations
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  checkAutoApproval,
  checkFilePermission,
  batchCheckFilePermissions,
  checkMcpApproval,
  createDefaultFilePermissionSettings,
  createDefaultMcpApprovalSettings,
  type AutoApprovalDecision,
  type CheckAutoApprovalParams,
} from "@wf-agent/sdk/services";

/**
 * Approval Adapter
 * Provides CLI-friendly access to auto-approval configuration and testing
 */
export class ApprovalAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Check auto-approval for a tool execution
   * @param params Check parameters (options, tool, context)
   */
  async checkApproval(params: CheckAutoApprovalParams): Promise<AutoApprovalDecision> {
    return this.executeWithErrorHandling(async () => {
      return checkAutoApproval(params);
    }, "Check auto-approval decision");
  }

  /**
   * Check file permission for a given operation
   * @param filePath File path to check
   * @param operation File operation type (read/write/delete)
   */
  async checkFile(filePath: string, operation: string): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const settings = createDefaultFilePermissionSettings();
      const result = checkFilePermission(filePath, operation as any, settings);
      return result as unknown as Record<string, unknown>;
    }, `Check file permission for "${filePath}"`);
  }

  /**
   * Batch check file permissions
   * @param files Array of {path, operation} pairs
   */
  async batchCheckFiles(
    files: Array<{ path: string; operation: string }>,
  ): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const settings = createDefaultFilePermissionSettings();
      const results = batchCheckFilePermissions(files as any, settings);
      const mapped: Record<string, unknown> = {};
      results.forEach((value, key) => {
        mapped[key] = value;
      });
      return mapped;
    }, "Batch check file permissions");
  }

  /**
   * Check MCP tool approval
   * @param serverName MCP server name
   * @param toolName Tool name
   */
  async checkMcpTool(serverName: string, toolName: string): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const settings = createDefaultMcpApprovalSettings();
      const decision = checkMcpApproval({
        settings,
        request: {
          serverName,
          toolName,
          parameters: {},
        } as any,
      });
      return decision as unknown as Record<string, unknown>;
    }, `Check MCP approval for "${serverName}/${toolName}"`);
  }

  /**
   * Get default file permission settings
   */
  async getDefaultFilePermissions(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const settings = createDefaultFilePermissionSettings();
      return settings as unknown as Record<string, unknown>;
    }, "Get default file permission settings");
  }

  /**
   * Get default MCP approval settings
   */
  async getDefaultMcpApproval(): Promise<Record<string, unknown>> {
    return this.executeWithErrorHandling(async () => {
      const settings = createDefaultMcpApprovalSettings();
      return settings as unknown as Record<string, unknown>;
    }, "Get default MCP approval settings");
  }
}
