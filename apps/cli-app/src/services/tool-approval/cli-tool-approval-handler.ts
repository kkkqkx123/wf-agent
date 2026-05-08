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
    console.log("\n========================================");
    console.log("  TOOL APPROVAL REQUEST");
    console.log("========================================");

    // Show batch context if available
    if (request.batchId) {
      console.log(`\nBatch ID: ${request.batchId}`);
      if (request.toolIndex !== undefined && request.totalTools !== undefined) {
        console.log(`Progress: Tool ${request.toolIndex + 1} of ${request.totalTools}`);
      }

      if (request.pendingQueue && request.pendingQueue.length > 0) {
        console.log("\nRemaining tools in queue:");
        request.pendingQueue.forEach((tc, idx) => {
          const name = tc.function?.name || "unknown";
          console.log(`  ${idx + 1}. ${name}`);
        });
      }
    }

    // Show tool details
    console.log(`\nTool Name: ${request.toolCall.function?.name || "unknown"}`);
    console.log(`Tool Call ID: ${request.toolCall.id}`);

    // Display parameters
    try {
      const args = JSON.parse(request.toolCall.function?.arguments || "{}");
      console.log("\nParameters:");
      console.log(JSON.stringify(args, null, 2));
    } catch (e) {
      console.log(`Arguments: ${request.toolCall.function?.arguments}`);
    }

    // Prompt user for decision
    console.log("\n----------------------------------------");
    console.log("Approve this tool? [y/n/edit/skip]");
    console.log("  y    = approve and continue");
    console.log("  n    = reject and stop");
    console.log("  edit = modify parameters");
    console.log("  skip = skip this tool, continue with next");
    console.log("----------------------------------------");

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
    console.log("\n----------------------------------------");
    console.log("Enter new parameters as JSON");
    console.log(`Current: ${currentArgs || "{}"}`);
    console.log("----------------------------------------");

    const jsonInput = await this.promptUser();

    try {
      return JSON.parse(jsonInput);
    } catch (e) {
      console.error("Invalid JSON, using original parameters");
      return currentArgs ? JSON.parse(currentArgs) : {};
    }
  }
}
