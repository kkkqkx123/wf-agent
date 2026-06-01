/**
 * TUI Output Handler
 *
 * Handles component messages routed to the TUI output target.
 * Maintains a message ring buffer for screen components to query
 * recent TUI-targeted messages.
 *
 * Screens subscribe to MessageBus directly (not through this handler)
 * for real-time message consumption.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";

/**
 * TUI Output Handler
 *
 * Maintains a ring buffer of recent TUI-targeted messages for
 * screen components to replay or inspect on demand.
 */
export class TUIOutputHandler implements OutputHandler {
  readonly target = OutputTarget.TUI;
  readonly name = "tui-output-handler";

  private messageBuffer: BaseComponentMessage[] = [];
  private readonly maxBufferSize: number;

  constructor(maxBufferSize: number = 100) {
    this.maxBufferSize = maxBufferSize;
  }

  supports(_message: BaseComponentMessage): boolean {
    return true;
  }

  handle(message: BaseComponentMessage): void {
    this.messageBuffer.push(message);
    if (this.messageBuffer.length > this.maxBufferSize) {
      this.messageBuffer.shift();
    }
  }

  getBuffer(): readonly BaseComponentMessage[] {
    return [...this.messageBuffer];
  }

  clearBuffer(): void {
    this.messageBuffer = [];
  }

  async flush(): Promise<void> {
    // TUI handler processes messages immediately; no buffer to flush
  }

  async close(): Promise<void> {
    this.messageBuffer = [];
  }
}