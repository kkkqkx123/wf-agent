/**
 * CLI Tool Approval Handler
 * Provides interactive command-line interface for tool approval requests
 */

import type {
  ToolApprovalHandler,
  ToolApprovalRequest,
  ToolApprovalResult,
} from "@wf-agent/types";
import readline from "readline";
import { getOutput } from "../../utils/output.js";

export class CLIToolApprovalHandler implements ToolApprovalHandler {
  /**
   * Request approval for tool execution
   * Displays tool information and prompts user for decision
   */
  async requestApproval(request: ToolApprovalRequest): Promise<ToolApprovalResult> {
    // Check if running in interactive mode
    if (!process.stdin.isTTY || !process.stdout.isTTY) {
      return {
        approved: false,
        toolCallId: request.toolCall.id,
        continueBatch: false,
        rejectionReason: "Non-interactive mode, approval not available",
      };
    }

    // Display approval request header
    const output = getOutput();
    output.output("\n========================================");
    output.output("  TOOL APPROVAL REQUEST");
    output.output("========================================");

    // Show batch context if available
    if (request.batchId) {
      output.output(`\nBatch ID: ${request.batchId}`);
      if (request.toolIndex !== undefined && request.totalTools !== undefined) {
        output.output(`Progress: Tool ${request.toolIndex + 1} of ${request.totalTools}`);
      }

      if (request.pendingQueue && request.pendingQueue.length > 0) {
        output.output("\nRemaining tools in queue:");
        request.pendingQueue.forEach((tc, idx) => {
          const name = tc.function?.name || "unknown";
          output.output(`  ${idx + 1}. ${name}`);
        });
      }
    }

    // Show tool details
    output.output(`\nTool Name: ${request.toolCall.function?.name || "unknown"}`);
    output.output(`Tool Call ID: ${request.toolCall.id}`);

    // Display parameters
    try {
      const args = JSON.parse(request.toolCall.function?.arguments || "{}");
      output.output("\nParameters:");
      output.output(JSON.stringify(args, null, 2));
    } catch (_e) {
      output.output(`Arguments: ${request.toolCall.function?.arguments}`);
    }

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
          toolCallId: request.toolCall.id,
          continueBatch: true,
        };

      case "n":
      case "no":
        return {
          approved: false,
          toolCallId: request.toolCall.id,
          continueBatch: false,
          rejectionReason: "User rejected",
        };

      case "edit": {
        const editedParams = await this.promptEditParameters(
          request.toolCall.function?.arguments,
        );
        return {
          approved: true,
          toolCallId: request.toolCall.id,
          editedParameters: editedParams,
          continueBatch: true,
        };
      }

      case "skip":
        return {
          approved: false,
          toolCallId: request.toolCall.id,
          continueBatch: true, // Skip but continue batch
        };

      default:
        return {
          approved: false,
          toolCallId: request.toolCall.id,
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
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
      });

      rl.question("> ", (answer) => {
        rl.close();
        resolve(answer.trim());
      });

      rl.on("SIGINT", () => {
        rl.close();
        reject(new Error("User cancelled"));
      });
    });
  }

  /**
   * Prompt user to edit tool parameters as JSON
   */
  private async promptEditParameters(
    currentArgs: string | undefined,
  ): Promise<Record<string, unknown>> {
    const output = getOutput();
    output.output("\n----------------------------------------");
    output.output("Enter new parameters as JSON");
    output.output(`Current: ${currentArgs || "{}"}`);
    output.output("----------------------------------------");

    const jsonInput = await this.promptUser();

    try {
      return JSON.parse(jsonInput);
    } catch (_e) {
      output.error("Invalid JSON, using original parameters");
      return currentArgs ? JSON.parse(currentArgs) : {};
    }
  }
}
