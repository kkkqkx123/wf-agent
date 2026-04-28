/**
 * CLI Human Relay Handler
 * Human input processing implementation in command line environment
 */

import type {
  HumanRelayHandler,
  HumanRelayRequest,
  HumanRelayResponse,
  HumanRelayContext,
} from "@wf-agent/types";
import * as readline from "readline";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * CLI Human Relay Handler
 * Handles human relay requests in CLI environment
 */
export class CLIHumanRelayHandler implements HumanRelayHandler {
  async handle(
    request: HumanRelayRequest,
    context: HumanRelayContext,
  ): Promise<HumanRelayResponse> {
    // 1. Display prompt information
    output.infoLog("\n╔════════════════════════════════════════════════════════════╗");
    output.infoLog("║                 HUMAN RELAY REQUEST                        ║");
    output.infoLog("╚════════════════════════════════════════════════════════════╝");

    output.infoLog(`\nRequest ID: ${request.requestId}`);
    output.infoLog(`Timeout: ${request.timeout}ms`);

    // 2. Display conversation history
    if (request.messages.length > 0) {
      output.infoLog("\n--- Conversation History ---");
      for (const msg of request.messages) {
        const role = msg.role.toUpperCase();
        const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);
        const truncated = content.length > 200 ? content.substring(0, 200) + "..." : content;
        output.infoLog(`[${role}]: ${truncated}`);
      }
    }

    // 3. Display current prompt
    output.infoLog("\n--- Current Prompt ---");
    output.infoLog(request.prompt);
    output.infoLog("\n--- Please Enter Your Response (Empty line to finish, Ctrl+C to cancel) ---");

    // 4. Read user input
    const content = await this.promptUser();

    // 5. Return response
    return {
      requestId: request.requestId,
      content,
      timestamp: Date.now(),
    };
  }

  /**
   * Prompt user for input
   * Supports multi-line input, empty line to finish
   */
  private promptUser(): Promise<string> {
    return new Promise((resolve, reject) => {
      const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,
      });

      let content = "";
      let isFirstLine = true;

      rl.setPrompt("> ");
      rl.prompt();

      rl.on("line", line => {
        if (line.trim() === "" && !isFirstLine) {
          // Empty line after content means finish
          rl.close();
          resolve(content.trim());
        } else {
          if (!isFirstLine) {
            content += "\n";
          }
          content += line;
          isFirstLine = false;
          rl.prompt();
        }
      });

      rl.on("SIGINT", () => {
        rl.close();
        reject(new Error("User cancelled"));
      });

      rl.on("close", () => {
        if (isFirstLine) {
          // No input provided
          resolve("(No response provided)");
        }
      });
    });
  }
}
