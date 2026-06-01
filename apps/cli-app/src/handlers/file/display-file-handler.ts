/**
 * Display File Handler
 * 
 * Handles display file IO operations (human-readable output).
 * Manages output.md aggregation and execution logging.
 */

import type { OutputHandler, BaseComponentMessage } from "@wf-agent/types";
import { OutputTarget, AgentMessageType, WorkflowExecutionMessageType, SystemMessageType } from "@wf-agent/types";
import type { DisplayOutputService, DisplaySection } from "../../services/io/index.js";
import { createContextualLogger } from "@wf-agent/sdk/utils";
import type {
  AgentToolResultData,
  WorkflowExecutionNodeData,
  AgentCheckpointData,
  AgentIterationData,
  AgentHumanRelayRequestData,
  AgentHumanRelayResponseData,
  WorkflowExecutionForkBranchData,
} from "@wf-agent/types";

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
   * Check if this handler supports the given message
   * Handles tool results, workflow events, checkpoints, and iterations
   */
  supports(message: BaseComponentMessage): boolean {
    const supportedTypes: string[] = [
      AgentMessageType.TOOL_RESULT,
      WorkflowExecutionMessageType.NODE_START,
      WorkflowExecutionMessageType.NODE_END,
      WorkflowExecutionMessageType.NODE_ERROR,
      AgentMessageType.HUMAN_RELAY_REQUEST,
      AgentMessageType.HUMAN_RELAY_RESPONSE,
      AgentMessageType.HUMAN_RELAY_TIMEOUT,
      AgentMessageType.HUMAN_RELAY_CANCEL,
      WorkflowExecutionMessageType.FORK_BRANCH_START,
      WorkflowExecutionMessageType.FORK_BRANCH_END,
      AgentMessageType.CHECKPOINT_CREATE,
      AgentMessageType.CHECKPOINT_RESTORE,
      AgentMessageType.ITERATION_START,
      AgentMessageType.ITERATION_END,
      SystemMessageType.ERROR,
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
      case AgentMessageType.TOOL_RESULT:
        return this.createToolResultSection(message);

      case WorkflowExecutionMessageType.NODE_START:
        return this.createNodeStartSection(message);

      case WorkflowExecutionMessageType.NODE_END:
        return this.createNodeEndSection(message);

      case AgentMessageType.CHECKPOINT_CREATE:
        return this.createCheckpointSection(message);

      case AgentMessageType.CHECKPOINT_RESTORE:
        return this.createCheckpointRestoreSection(message);

      case AgentMessageType.ITERATION_START:
        return this.createIterationSection(message);

      case AgentMessageType.ITERATION_END:
        return this.createIterationEndSection(message);

      case AgentMessageType.HUMAN_RELAY_REQUEST:
        return this.createHumanRelayRequestSection(message);

      case AgentMessageType.HUMAN_RELAY_RESPONSE:
        return this.createHumanRelayResponseSection(message);

      case AgentMessageType.HUMAN_RELAY_TIMEOUT:
        return this.createHumanRelayTimeoutSection(message);

      case AgentMessageType.HUMAN_RELAY_CANCEL:
        return this.createHumanRelayCancelSection(message);

      case WorkflowExecutionMessageType.FORK_BRANCH_START:
        return this.createForkBranchStartSection(message);

      case WorkflowExecutionMessageType.FORK_BRANCH_END:
        return this.createForkBranchEndSection(message);

      case WorkflowExecutionMessageType.NODE_ERROR:
        return this.createNodeErrorSection(message);

      case SystemMessageType.ERROR:
        return this.createErrorSection(message);

      default:
        return null;
    }
  }

  /**
   * Create section for tool result
   */
  private createToolResultSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as AgentToolResultData;
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
    const data = message.data as WorkflowExecutionNodeData;
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
    const data = message.data as WorkflowExecutionNodeData;
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
    const data = message.data as AgentCheckpointData;
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
    const data = message.data as AgentIterationData;
    const iteration = data.iteration || 0;
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Iteration Started: ${iteration}`,
      content: `[${timestamp}] Iteration ${iteration} started`,
    };
  }

  /**
   * Create section for iteration end
   */
  private createIterationEndSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as AgentIterationData;
    const iteration = data.iteration || 0;
    const duration = data.duration ? `${data.duration}ms` : "N/A";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Iteration Completed: ${iteration}`,
      content: `[${timestamp}] Iteration ${iteration} completed\nDuration: ${duration}`,
    };
  }

  /**
   * Create section for checkpoint restore
   */
  private createCheckpointRestoreSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as AgentCheckpointData;
    const checkpointId = data.checkpointId || "unknown";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Checkpoint Restored: ${checkpointId}`,
      content: `[${timestamp}] Checkpoint "${checkpointId}" restored`,
    };
  }

  /**
   * Create section for human relay request
   */
  private createHumanRelayRequestSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as AgentHumanRelayRequestData;
    const requestId = data.requestId || "unknown";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Human Relay Request: ${requestId}`,
      content: `[${timestamp}] Human relay request received\nPrompt: ${data.prompt || "N/A"}`,
    };
  }

  /**
   * Create section for human relay response
   */
  private createHumanRelayResponseSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as AgentHumanRelayResponseData;
    const requestId = data.requestId || "unknown";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Human Relay Response: ${requestId}`,
      content: `[${timestamp}] Human relay response received (${data.responseTime || "N/A"}ms)`,
    };
  }

  /**
   * Create section for human relay timeout
   */
  private createHumanRelayTimeoutSection(message: BaseComponentMessage): DisplaySection {
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: "Human Relay Timeout",
      content: `[${timestamp}] Human relay request timed out`,
    };
  }

  /**
   * Create section for human relay cancel
   */
  private createHumanRelayCancelSection(message: BaseComponentMessage): DisplaySection {
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: "Human Relay Cancelled",
      content: `[${timestamp}] Human relay request was cancelled`,
    };
  }

  /**
   * Create section for fork branch start
   */
  private createForkBranchStartSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as WorkflowExecutionForkBranchData;
    const branchIndex = data.branchIndex ?? 0;
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Fork Branch Started: ${branchIndex}`,
      content: `[${timestamp}] Fork branch ${branchIndex} started`,
    };
  }

  /**
   * Create section for fork branch end
   */
  private createForkBranchEndSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as WorkflowExecutionForkBranchData;
    const branchIndex = data.branchIndex ?? 0;
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Fork Branch Completed: ${branchIndex}`,
      content: `[${timestamp}] Fork branch ${branchIndex} completed`,
    };
  }

  /**
   * Create section for node error
   */
  private createNodeErrorSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as WorkflowExecutionNodeData;
    const nodeId = data.nodeId || "unknown";
    const error = data.error || "Unknown error";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: `Node Error: ${nodeId}`,
      content: `[${timestamp}] Node "${nodeId}" failed\nError: ${error}`,
    };
  }

  /**
   * Create section for error
   */
  private createErrorSection(message: BaseComponentMessage): DisplaySection {
    const data = message.data as { message?: string };
    const errorMessage = data.message || "Unknown error";
    const timestamp = new Date(message.timestamp).toLocaleTimeString();

    return {
      title: "Error",
      content: `[${timestamp}] ❌ Error occurred: ${errorMessage}`,
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
