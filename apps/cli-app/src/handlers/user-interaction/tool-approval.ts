/**
 * CLI Tool Approval Handler
 * Provides interactive command-line interface for tool approval requests.
 * Migrated to user-interaction directory for unified interaction management.
 */

import readline from "readline";
import type { 
  ToolApprovalRequestData,
  ToolApprovalResponseData,
  ToolApprovalHandler,
  ToolApprovalRequest,
  ToolApprovalResult
} from "@wf-agent/types";
import { getOutput } from "../../utils/output.js";

export class CLIToolApprovalHandler implements ToolApprovalHandler {
  private rl: readline.Interface;

  constructor() {
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
    });
  }

  /**
   * Request approval for tool execution (implements ToolApprovalHandler)
   */
  async requestApproval(request: ToolApprovalRequest): Promise<ToolApprovalResult> {
    // Extract structured data from the request for our handler
    const data: ToolApprovalRequestData = {
      toolCallId: request.toolCall.id,
      toolName: request.toolCall.function?.name || "unknown",
      parameters: JSON.parse(request.toolCall.function?.arguments || "{}"),
      batchId: request.batchId,
      toolIndex: request.toolIndex,
      totalTools: request.totalTools,
      pendingQueue: request.pendingQueue?.map(tc => ({
        id: tc.id,
        name: tc.function?.name || "unknown",
        arguments: tc.function?.arguments
      })),
      // Pass configuration fields if available
      timeout: (request as any).timeout,
      securityPreset: (request as any).securityPreset,
    };

    const response = await this.handle(data);

    return {
      ...response,
      toolCallId: request.toolCall.id,
    };
  }

  /**
   * Handle the tool approval request and return structured response
   */
  public async handle(data: ToolApprovalRequestData): Promise<ToolApprovalResponseData> {
    // Check if running in interactive mode
    if (!process.stdin.isTTY || !process.stdout.isTTY) {
      return {
        approved: false,
        rejectionReason: "Non-interactive mode, approval not available",
      };
    }

    const output = getOutput();
    output.output("\n========================================");
    output.output("  TOOL APPROVAL REQUEST");
    output.output("========================================");

    // Show batch context if available
    if (data.batchId) {
      output.output(`\nBatch ID: ${data.batchId}`);
      if (data.toolIndex !== undefined && data.totalTools !== undefined) {
        output.output(`Progress: Tool ${data.toolIndex + 1} of ${data.totalTools}`);
      }

      if (data.pendingQueue && data.pendingQueue.length > 0) {
        output.output("\nRemaining tools in queue:");
        data.pendingQueue.forEach((tc, idx) => {
          output.output(`  ${idx + 1}. ${tc.name}`);
        });
      }
    }

    // Show tool details
    output.output(`\nTool Name: ${data.toolName}`);
    if (data.toolDescription) {
      output.output(`Description: ${data.toolDescription}`);
    }

    // Display parameters
    output.output("\nParameters:");
    output.output(JSON.stringify(data.parameters, null, 2));

    // Prompt user for decision
    output.output("\n----------------------------------------");
    output.output("Approve this tool? [y/n/edit/skip]");
    output.output("  y    = approve and continue");
    output.output("  n    = reject and stop");
    output.output("  edit = modify parameters");
    output.output("  skip = skip this tool, continue with next");
    output.output("----------------------------------------");

    const decision = await this.promptUser();

    // Process decision
    switch (decision.toLowerCase()) {
      case "y":
      case "yes":
        return {
          approved: true,
          continueBatch: true,
        };

      case "n":
      case "no":
        return {
          approved: false,
          continueBatch: false,
          rejectionReason: "User rejected",
        };

      case "edit": {
        const editedParams = await this.promptEditParameters(data.parameters);
        return {
          approved: true,
          editedParameters: editedParams,
          continueBatch: true,
        };
      }

      case "skip":
        return {
          approved: false,
          continueBatch: true, // Skip but continue batch
        };

      default:
        return {
          approved: false,
          continueBatch: false,
          rejectionReason: "Invalid response",
        };
    }
  }

  /**
   * Prompt user for single-line input
   */
  private promptUser(): Promise<string> {
    return new Promise((resolve, reject) => {
      this.rl.question("> ", (answer) => {
        resolve(answer.trim());
      });

      this.rl.on("SIGINT", () => {
        reject(new Error("User cancelled"));
      });
    });
  }

  /**
   * Prompt user to edit tool parameters as JSON
   */
  private async promptEditParameters(
    currentParams: Record<string, unknown>,
  ): Promise<Record<string, unknown>> {
    const output = getOutput();
    output.output("\n----------------------------------------");
    output.output("Enter new parameters as JSON");
    output.output(`Current: ${JSON.stringify(currentParams)}`);
    output.output("----------------------------------------");

    const jsonInput = await this.promptUser();

    try {
      return JSON.parse(jsonInput);
    } catch (_e) {
      output.error("Invalid JSON, using original parameters");
      return currentParams;
    }
  }

  /**
   * Cleanup resources
   */
  public cleanup(): void {
    this.rl.close();
  }
}
