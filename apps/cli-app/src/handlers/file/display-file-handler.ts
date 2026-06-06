/**
 * Display File Handler
 * 
 * Handles display file IO operations (human-readable output).
 * Manages output.md aggregation and execution logging.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";
import type { DisplayOutputService, DisplaySection } from "../../services/io/index.js";
import { formatMessage } from "../../formatters/display-formatter.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";

/**
 * Display File Handler
 * 
 * Processes messages that require display file output.
 * Aggregates execution information into output.md for human reading.
 */
export class DisplayFileHandler implements OutputHandler {
  readonly target = OutputTarget.FILE_DISPLAY;
  readonly name = "file_display";

  // Buffer sections for batching writes
  private buffer: Map<string, DisplaySection[]> = new Map();
  private flushTimers: Map<string, NodeJS.Timeout> = new Map(); // Per-session timers
  private readonly FLUSH_INTERVAL = 2000; // Flush every 2 seconds
  private logger = createContextualLogger({ component: "DisplayFileHandler" });

  constructor(private displayOutputService: DisplayOutputService) {}

  /**
   * Handle the message by updating display files
   * Buffers and batches writes to avoid frequent file I/O
   */
  async handle(message: BaseComponentMessage): Promise<void> {
    const sessionId = message.entity?.id || this.generateSessionId();

    // Initialize buffer for this session if needed
    if (!this.buffer.has(sessionId)) {
      this.buffer.set(sessionId, []);
    }

    const sections = this.buffer.get(sessionId)!;

    // Create section based on message type
    const section = this.createSectionFromMessage(message);
    if (section) {
      sections.push(section);
    }

    // Schedule flush
    this.scheduleFlush(sessionId);
  }

  /**
   * Create a display section from a message by delegating to the shared formatter.
   */
  private createSectionFromMessage(message: BaseComponentMessage): DisplaySection | null {
    const entry = formatMessage(message);
    if (!entry) return null;

    const timeStr = new Date(entry.timestamp).toLocaleTimeString();
    const levelIcon = { info: "•", success: "✓", warning: "⚠", error: "✗" }[entry.level];

    return {
      title: entry.title,
      content: `[${timeStr}] ${levelIcon} ${entry.summary}${entry.detail ? `\n${entry.detail}` : ""}`,
    };
  }

  /**
   * Schedule a flush operation for buffered sections of a specific session
   * @param sessionId Session identifier
   */
  private scheduleFlush(sessionId: string): void {
    // Clear existing timer for this session if any
    const existingTimer = this.flushTimers.get(sessionId);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    // Schedule new flush for this specific session
    const timer = setTimeout(() => {
      this.flushSession(sessionId);
    }, this.FLUSH_INTERVAL);
    
    this.flushTimers.set(sessionId, timer);
  }

  /**
   * Flush buffered sections for a specific session to display files
   * @param sessionId Session identifier
   */
  async flushSession(sessionId: string): Promise<void> {
    const sections = this.buffer.get(sessionId);
    if (!sections || sections.length === 0) {
      return;
    }

    try {
      await this.displayOutputService.updateOutput({
        sessionId,
        sections,
        append: true,
      });

      // Clear buffer after successful write
      this.buffer.set(sessionId, []);
      
      // Clean up timer
      this.flushTimers.delete(sessionId);
      
      this.logger.debug(`Flushed display output for session`, { sessionId, sectionCount: sections.length });
    } catch (error) {
      this.logger.error(`Failed to flush display output for session`, {}, { 
        sessionId,
        error: error instanceof Error ? error.message : String(error) 
      });
    }
  }

  /**
   * Flush all buffered sections to display files
   */
  async flushAll(): Promise<void> {
    const sessionIds = Array.from(this.buffer.keys());
    
    // Flush all sessions in parallel
    await Promise.all(
      sessionIds.map(sessionId => this.flushSession(sessionId))
    );
  }

  /**
   * Flush buffered sections immediately
   */
  async flush(): Promise<void> {
    await this.flushAll();
  }

  /**
   * Close the handler and flush remaining buffers
   */
  async close(): Promise<void> {
    // Clear all pending timers
    for (const timer of this.flushTimers.values()) {
      clearTimeout(timer);
    }
    this.flushTimers.clear();
    
    await this.flushAll();
  }

  /**
   * Generate a session ID if not provided in message
   * @returns Session identifier
   */
  private generateSessionId(): string {
    return `session-${Date.now()}`;
  }
}
