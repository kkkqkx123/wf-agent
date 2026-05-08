/**
 * TUI Message Handler
 * 
 * Handles component messages for display in the Terminal User Interface.
 * Only handles lightweight, high-frequency messages that need real-time feedback.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";
import type { TUI } from "../../tui/core/tui.js";

/**
 * TUI Handler
 * 
 * Processes messages for TUI display. White-lists specific message types
 * to keep the TUI lightweight and responsive.
 */
export class TUIHandler implements OutputHandler {
  readonly target = OutputTarget.TUI;
  readonly name = "tui";

  constructor(private tui: TUI) {}

  /**
   * Check if this handler supports the given message
   * Only handles lightweight messages suitable for TUI display
   */
  supports(message: BaseComponentMessage): boolean {
    // White-list of message types for TUI
    const supportedTypes = new Set([
      "agent.llm.stream",
      "agent.tool.call_start",
      "agent.tool.call_end",
      "agent.human_relay.request",
      "agent.iteration.start",
      "workflow-execution.node.start",
      "workflow-execution.node.end",
      "system.error",
    ]);

    return supportedTypes.has(message.type);
  }

  /**
   * Handle the message for TUI display
   * Delegates to appropriate screen/component based on message type
   */
  async handle(message: BaseComponentMessage): Promise<void> {
    // Note: Actual rendering is handled by subscribed screens
    // This handler acts as a router to notify the TUI system
    
    // For now, we log to console - actual implementation will integrate with TUI screens
    switch (message.type) {
      case "agent.llm.stream":
        // Streaming LLM output - should be displayed in AgentScreen
        break;

      case "agent.tool.call_start":
        // Tool call started - show brief notification
        break;

      case "agent.tool.call_end":
        // Tool call completed - show completion status
        break;

      case "agent.human_relay.request":
        // Human relay requested - trigger overlay in AgentScreen
        break;

      case "agent.iteration.start":
        // New iteration started - update status panel
        break;

      case "workflow-execution.node.start":
        // Workflow node started - update workflow screen
        break;

      case "workflow-execution.node.end":
        // Workflow node ended - update workflow screen
        break;

      case "system.error":
        // System error - show error notification
        break;
    }
  }
}
