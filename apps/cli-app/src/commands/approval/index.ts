/**
 * Approval Command Group
 * Commands for inspecting auto-approval rules and testing approval decisions
 */

import { Command } from "commander";
import { ApprovalAdapter } from "../../adapters/approval-adapter.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";

const router = getRouter();

/**
 * Create Approval Command Group
 */
export function createApprovalCommands(): Command {
  const approvalCmd = new Command("approval").description(
    "Inspect auto-approval rules and test approval decisions",
  );

  // --- approval config show ---
  approvalCmd
    .command("config")
    .description("Show default auto-approval file permission settings")
    .action(async () => {
      try {
        const adapter = new ApprovalAdapter();
        const settings = await adapter.getDefaultFilePermissions();

        router.render(settings, {
          type: "detail",
          entity: "approval-config",
          format: () => JSON.stringify(settings, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "approval-config" });
      }
    });

  // Subcommand: approval file
  const fileCmd = new Command("file").description("Check file-level permissions");

  // --- approval file check <path> ---
  fileCmd
    .command("check <path>")
    .description("Check file permission for a given operation")
    .option("-o, --operation <type>", "File operation: read, write, delete", "read")
    .action(async (filePath: string, options: { operation: string }) => {
      try {
        const adapter = new ApprovalAdapter();
        const result = await adapter.checkFile(filePath, options.operation);

        router.render(result, {
          type: "detail",
          entity: "approval-file-check",
          format: () => JSON.stringify(result, null, 2),
        });
      } catch (error) {
        handleError(error, {
          operation: "approval-file-check",
          additionalInfo: { filePath, operation: options.operation },
        });
      }
    });

  // --- approval file batch <paths...> ---
  fileCmd
    .command("batch <paths...>")
    .description("Batch check file permissions for multiple paths")
    .option("-o, --operation <type>", "File operation: read, write, delete", "read")
    .action(async (paths: string[], options: { operation: string }) => {
      try {
        const files = paths.map((p) => ({ path: p, operation: options.operation }));
        const adapter = new ApprovalAdapter();
        const results = await adapter.batchCheckFiles(files);

        router.render(results, {
          type: "detail",
          entity: "approval-file-batch",
          format: () => JSON.stringify(results, null, 2),
        });
      } catch (error) {
        handleError(error, {
          operation: "approval-file-batch",
          additionalInfo: { paths, operation: options.operation },
        });
      }
    });

  approvalCmd.addCommand(fileCmd);

  // Subcommand: approval mcp
  const mcpCmd = new Command("mcp").description("MCP approval configuration");

  // --- approval mcp config ---
  mcpCmd
    .command("config")
    .description("Show default MCP approval settings")
    .action(async () => {
      try {
        const adapter = new ApprovalAdapter();
        const settings = await adapter.getDefaultMcpApproval();

        router.render(settings, {
          type: "detail",
          entity: "approval-mcp-config",
          format: () => JSON.stringify(settings, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "approval-mcp-config" });
      }
    });

  // --- approval mcp check <serverName> <toolName> ---
  mcpCmd
    .command("check <serverName> <toolName>")
    .description("Check MCP tool approval decision")
    .action(async (serverName: string, toolName: string) => {
      try {
        const adapter = new ApprovalAdapter();
        const decision = await adapter.checkMcpTool(serverName, toolName);

        router.render(decision, {
          type: "detail",
          entity: "approval-mcp-check",
          format: () => JSON.stringify(decision, null, 2),
        });
      } catch (error) {
        handleError(error, {
          operation: "approval-mcp-check",
          additionalInfo: { serverName, toolName },
        });
      }
    });

  approvalCmd.addCommand(mcpCmd);

  // --- approval defaults ---
  approvalCmd
    .command("defaults")
    .description("Show default file permission and MCP approval settings")
    .action(async () => {
      try {
        const adapter = new ApprovalAdapter();
        const [filePerms, mcpApproval] = await Promise.all([
          adapter.getDefaultFilePermissions(),
          adapter.getDefaultMcpApproval(),
        ]);

        const combined = { filePermissions: filePerms, mcpApproval };

        router.render(combined, {
          type: "detail",
          entity: "approval-defaults",
          format: () => JSON.stringify(combined, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "approval-defaults" });
      }
    });

  return approvalCmd;
}
