/**
 * Display File Handler
 * 
 * Handles display file IO operations (human-readable output).
 * Manages output.md aggregation and execution logging.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget } from "@wf-agent/types";
import type { DisplayOutputService, DisplaySection } from "../../services/io/index.js";
import { createContextualLogger } from "@wf-agent/sdk";

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
  private flushTimer: NodeJS.Timeout | null = null;
  private readonly FLUSH_INTERVAL = 2000; // Flush every 2 seconds
  private logger = createContextualLogger({ component: "DisplayFileHandler" });

  constructor(private displayOutputService: DisplayOutputService) {}

  /**
   * Check if this handler supports the given message
   * Handles tool results, workflow events, checkpoints, and iterations
   */
  supports(message: BaseComponentMessage): boolean {
    const supportedTypes = [
      "agent.tool.result",
      "workflow-execution.node.start",
      "workflow-execution.node.end",
      "workflow-execution.checkpoint.create",
      "agent.iteration.start",
      "system.error",
    ];

    return supportedTypes.includes(message.type);
  }

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
   * Create a display section from a message
   * @param message Component message
   * @returns Display section or null
   */
  private createSectionFromMessage(message: BaseComponentMessage): DisplaySection | null {
    switch (message.type) {
      case "agent.tool.result":
        return this.createToolResultSection(message);

      case "workflow-execution.node.start":
        return this.createNodeStartSection(message);

      case "workflow-execution.node.end":
        return this.createNodeEndSection(message);

      case "workflow-execution.checkpoint.create":
        return this.createCheckpointSection(message);

      case "agent.iteration.start":
        return this.createIterationSection(message);

      case "system.error":
        return this.createErrorSection(message);

      default:
        return null;
    }
  }

  /**
   * Create section for tool result
   */
  private createToolResultSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as any;
    const toolName = data.toolName || "unknown";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Tool Call Result: ${toolName}`,
      content: `[${timestamp}] Tool "${toolName}" execution completed\nResult saved`,
    };
  }

  /**
   * Create section for node start
   */
  private createNodeStartSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as any;
    const nodeId = data.nodeId || "unknown";
    const nodeType = data.nodeType || "unknown";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Node Started: ${nodeId}`,
      content: `[${timestamp}] Node "${nodeId}" (${nodeType}) started execution`,
    };
  }

  /**
   * Create section for node end
   */
  private createNodeEndSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as any;
    const nodeId = data.nodeId || "unknown";
    const duration = data.duration ? `${data.duration}ms` : "N/A";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Node Completed: ${nodeId}`,
      content: `[${timestamp}] Node "${nodeId}" execution completed\nDuration: ${duration}`,
    };
  }

  /**
   * Create section for checkpoint
   */
  private createCheckpointSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as any;
    const checkpointId = data.checkpointId || "unknown";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Checkpoint Created: ${checkpointId}`,
      content: `[${timestamp}] Checkpoint "${checkpointId}" created`,
    };
  }

  /**
   * Create section for iteration start
   */
  private createIterationSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as any;
    const iteration = data.iteration || 0;
    const maxIterations = data.maxIterations || "∞";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Iteration Started: ${iteration}/${maxIterations}`,
      content: `[${timestamp}] Iteration ${iteration} started (max ${maxIterations} iterations)`,
    };
  }

  /**
   * Create section for error
   */
  private createErrorSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as any;
    const errorMessage = data.message || "Unknown error";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: "Error",
      content: `[${timestamp}] ❌ Error occurred: ${errorMessage}`,
    };
  }

  /**
   * Schedule a flush operation for buffered sections
   * @param sessionId Session identifier
   */
  private scheduleFlush(sessionId: string): void {
    // Clear existing timer if any
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
    }

    // Schedule new flush
    this.flushTimer = setTimeout(() => {
      this.flushAll();
    }, this.FLUSH_INTERVAL);
  }

  /**
   * Flush all buffered sections to display files
   */
  async flushAll(): Promise<void> {
    for (const [sessionId, sections] of this.buffer.entries()) {
      if (sections.length > 0) {
        try {
          await this.displayOutputService.updateOutput({
            sessionId,
            sections,
            append: true,
          });

          // Clear buffer after successful write
          this.buffer.set(sessionId, []);
        } catch (error) {
          this.logger.error(`Failed to flush display output for session`, {}, { 
            sessionId,
            error: error instanceof Error ? error.message : String(error) 
          });
        }
      }
    }

    this.flushTimer = null;
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
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
    }
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
