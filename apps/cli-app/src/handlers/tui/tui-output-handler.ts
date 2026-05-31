/**
 * TUI Output Handler
 *
 * Handles component messages routed to the TUI output target.
 * Maintains a message ring buffer and provides subscriber pattern
 * for screen components to consume TUI-targeted messages.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";
import { createContextualLogger } from "@wf-agent/sdk/utils";

/**
 * TUI message subscriber callback
 */
export type TUIMessageSubscriber = (message: BaseComponentMessage) => void;

/**
 * TUI Output Handler
 *
 * Bridges the gap between routing rules and TUI screen display.
 * Screens can either subscribe directly to MessageBus (existing pattern)
 * or subscribe to this handler for TUI-targeted messages only.
 */
export class TUIOutputHandler implements OutputHandler {
  readonly target = OutputTarget.TUI;
  readonly name = "tui-output-handler";

  private messageBuffer: BaseComponentMessage[] = [];
  private readonly maxBufferSize: number;
  private subscribers: Set<TUIMessageSubscriber> = new Set();
  private logger = createContextualLogger({ component: "TUIOutputHandler" });

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

    for (const subscriber of this.subscribers) {
      try {
        subscriber(message);
      } catch (err) {
        this.logger.error("TUI subscriber failed", { error: err });
      }
    }
  }

  subscribe(subscriber: TUIMessageSubscriber): () => void {
    this.subscribers.add(subscriber);
    return () => {
      this.subscribers.delete(subscriber);
    };
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
    this.subscribers.clear();
    this.messageBuffer = [];
  }
}