/**
 * TUI Human Relay Handler
 * 
 * Handles Human Relay requests in TUI mode using file-based workflow.
 * Integrates with HumanRelayService for functional file operations.
 */

import type { HumanRelayHandler, HumanRelayRequest, HumanRelayResponse, HumanRelayContext } from "@wf-agent/types";
import type { HumanRelayService } from "../../services/io/index.js";
import type { TUI } from "../core/tui.js";
import { Box, Text, Spacer } from "../core/index.js";

/**
 * TUI Human Relay Handler
 * 
 * Implements file-based Human Relay workflow for TUI mode:
 * 1. Writes prompt to functional file (human-relay-output.txt)
 * 2. Shows instructions overlay in TUI
 * 3. Watches input file (human-relay-input.txt) for user response
 * 4. Returns response when file is updated
 */
export class TUIHumanRelayHandler implements HumanRelayHandler {
  private tui: TUI;
  private humanRelayService: HumanRelayService;

  constructor(tui: TUI, humanRelayService: HumanRelayService) {
    this.tui = tui;
    this.humanRelayService = humanRelayService;
  }

  /**
   * Handle Human Relay request
   * @param request Human Relay request with prompt and messages
   * @param context Additional context (should include sessionId)
   * @returns Human Relay response
   */
  async handle(request: HumanRelayRequest, context: HumanRelayContext): Promise<HumanRelayResponse> {
    const sessionId = (context as unknown as Record<string, unknown>)["sessionId"] as string | undefined || request.requestId;

    // Step 1: Write prompt to functional file (pure text)
    await this.humanRelayService.writeOutput({
      sessionId,
      content: request.prompt,
    });

    // Step 2: Get file paths for display
    const paths = this.humanRelayService.getSessionPaths(sessionId);

    return new Promise((resolve, reject) => {
      // Create instruction overlay
      const overlay = new Box(1, 1);
      overlay.addChild(new Text("🤝 Human Relay Request", 0, 0));

      // Add request info
      overlay.addChild(new Text(`Request ID: ${request.requestId}`, 0, 0));
      overlay.addChild(new Text(`Timeout: ${request.timeout}ms`, 0, 0));
      overlay.addChild(new Spacer());

      // Show recent messages (last 3, truncated)
      if (request.messages.length > 0) {
        overlay.addChild(new Text("Recent Messages:", 0, 0));
        const recentMessages = request.messages.slice(-3);
        for (const msg of recentMessages) {
          const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);
          const truncated = content.length > 150 ? content.substring(0, 150) + "..." : content;
          overlay.addChild(new Text(`${msg.role}: ${truncated}`, 0, 0));
        }
        overlay.addChild(new Spacer());
      }

      // Instructions
      overlay.addChild(new Text("Instructions:", 0, 0));
      overlay.addChild(
        new Text(
          `1. View full prompt:\n   ${paths.output}\n\n` +
            `2. Copy to web LLM and get response\n\n` +
            `3. Paste response to:\n   ${paths.input}\n\n` +
            `4. Save file - execution continues automatically`,
          0,
          0,
        ),
      );
      overlay.addChild(new Spacer());

      overlay.addChild(new Text("⏳ Waiting for your response...", 0, 0));

      // Show overlay
      const handle = this.tui.showOverlay(overlay, {
        anchor: "center",
        nonCapturing: false,
      });

      // Step 3: Start file watcher
      this.humanRelayService.watchInput({
        sessionId,
        timeout: request.timeout,
        onResponse: (content: string) => {
          handle.hide();
          resolve({
            requestId: request.requestId,
            content,
            timestamp: Date.now(),
          });
        },
        onTimeout: () => {
          handle.hide();
          reject(new Error(`Human Relay timeout after ${request.timeout}ms`));
        },
      });
    });
  }
}
