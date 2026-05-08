/**
 * Functional File Handler
 * 
 * Handles functional file IO operations (program-to-program data exchange).
 * Specifically handles Human Relay requests by writing prompts to files.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";
import type { HumanRelayService } from "../io/index.js";

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
    return message.type === "agent.human_relay.request";
  }

  /**
   * Handle the message by writing to functional files
   * Writes Human Relay prompt to human-relay-output.txt
   */
  async handle(message: BaseComponentMessage): Promise<void> {
    if (message.type === "agent.human_relay.request") {
      const data = message.data as any;
      const { prompt } = data;

      // Extract session ID from entity context
      const sessionId = message.entity?.id || this.generateSessionId();

      // Write prompt to functional file (pure text)
      await this.humanRelayService.writeOutput({
        sessionId,
        content: prompt,
      });

      // Note: File watching is handled separately by TUIHumanRelayHandler
      // This handler only writes the output file
    }
  }

  /**
   * Generate a session ID if not provided in message
   * @returns Session identifier
   */
  private generateSessionId(): string {
    return `session-${Date.now()}`;
  }
}
