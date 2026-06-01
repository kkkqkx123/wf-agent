/**
 * Functional File Handler
 * 
 * Handles functional file IO operations (program-to-program data exchange).
 * Primarily processes Human Relay request messages by writing prompts to files.
 *
 * Note: For TUI mode, the TUIHumanRelayHandler is the sole writer of
 * human-relay-output.txt. This handler exists to support non-TUI modes.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget, AgentMessageType } from "@wf-agent/types";
import type { HumanRelayService } from "../../services/io/index.js";

/**
 * Functional File Handler
 * 
 * Processes messages that require functional file output.
 * Currently handles Human Relay request messages.
 */
export class FunctionalFileHandler implements OutputHandler {
  readonly target = OutputTarget.FILE_FUNCTIONAL;
  readonly name = "file_functional";

  constructor(private humanRelayService: HumanRelayService) {}

  /**
   * Check if this handler supports the given message
   * Only handles Human Relay request messages
   */
  supports(message: BaseComponentMessage): boolean {
    return message.type === AgentMessageType.HUMAN_RELAY_REQUEST;
  }

  /**
   * Handle the message by writing to functional files
   * Writes Human Relay prompt to human-relay-output.txt
   */
  async handle(message: BaseComponentMessage): Promise<void> {
    if (message.type === AgentMessageType.HUMAN_RELAY_REQUEST) {
      const data = message.data as { prompt: string };
      const { prompt } = data;

      // Extract session ID from entity context
      const sessionId = message.entity?.id || this.generateSessionId();

      // Write prompt to functional file (pure text)
      await this.humanRelayService.writeOutput({
        sessionId,
        content: prompt,
      });
    }
  }

  /**
   * Generate a session ID if not provided in message
   * @returns Session identifier
   */
  private generateSessionId(): string {
    return `session-${Date.now()}`;
  }

  async flush(): Promise<void> {
    // FunctionalFileHandler writes synchronously; no buffering needed
  }

  async close(): Promise<void> {
    // No resources to release
  }
}
