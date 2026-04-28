/**
 * WorkflowConversationSession - Workflow-specific Conversation Session
 * Extends ConversationSession to add Workflow-specific features (such as tool visibility, etc.)
 *
 * Core Responsibilities:
 * 1. Manage the message history of workflow executions (inherited from ConversationSession)
 * 2. Token statistics and event triggering (inherited from ConversationSession)
 * 3. Tool-specific message management (unique to Workflow)
 * 4. Batch-level message version control (supports snapshots and restoration)
 */

import type {
  LLMMessage,
  Tool,
  MessageOperationConfig,
  MessageOperationResult,
} from "@wf-agent/types";
import {
  ConversationSession,
  type ConversationSessionConfig,
} from "../../../core/messaging/conversation-session.js";
import type { ToolRegistry } from "../../../core/registry/tool-registry.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { generateToolDescriptionMessage } from "../../../resources/dynamic/prompts/fragments/available-tools.js";

const logger = createContextualLogger({ component: "WorkflowConversationSession" });

/**
 * Tool Availability Set
 */
export interface AvailableTools {
  initial: Set<string>;
  dynamic: Set<string>;
}

/**
 * WorkflowConversationSession Configuration
 */
export interface WorkflowConversationSessionConfig extends ConversationSessionConfig {
  /** Tool Services */
  toolService?: ToolRegistry;
  /** Collection of available tools */
  availableTools?: AvailableTools;
}

/**
 * WorkflowConversationSession class
 * Extends ConversationSession with Workflow-specific tool visibility management
 */
export class WorkflowConversationSession extends ConversationSession {
  private toolService?: ToolRegistry;
  private availableTools?: AvailableTools;

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: WorkflowConversationSessionConfig) {
    super(config);
    this.toolService = config.toolService;
    this.availableTools = config.availableTools;
  }

  // ============================================================
  // Tool Description: Message Management (specific to Workflow)
  // ============================================================

  /**
   * Check if a tool description message already exists.
   * @returns Whether a tool description message exists.
   */
  private hasToolDescriptionMessage(): boolean {
    const allMessages = this.getAllMessages();
    return allMessages.some(
      msg =>
        msg.role === "system" &&
        typeof msg.content === "string" &&
        msg.content.includes("AVAILABLE TOOLS"),
    );
  }

  /**
   * Get the description message of the initially available tools (excluding dynamicTools)
   * @returns The tool description message; if no initial tools are available, return null
   */
  getInitialToolDescriptionMessage(): LLMMessage | null {
    if (!this.availableTools || !this.toolService) {
      return null;
    }

    // Use only the initial set of tools.
    const initialToolIds = Array.from(this.availableTools.initial);
    if (initialToolIds.length === 0) {
      return null;
    }

    // Get the list of tool objects.
    const tools = initialToolIds
      .map(id => {
        try {
          return this.toolService!.getTool(id);
        } catch (e) {
          logger.debug(`Failed to get tool '${id}', skipping from tool description`, {
            toolId: id,
            context: "getInitialToolDescriptionMessage",
            error: getErrorOrNew(e),
          });
          return null;
        }
      })
      .filter((t): t is Tool => !!t);

    // Use the available-tools fragment generator to create tool description message
    return generateToolDescriptionMessage(tools);
  }

  /**
   * Start a new batch with initial tool description
   * If checkpoint storage is configured, automatically saves previous batch to checkpoint
   * @param boundaryIndex Batch boundary index
   * @param keepInMemory Number of recent batches to keep in memory (default: 2, only used when checkpoint is enabled)
   * @returns New batch number
   */
  async startNewBatchWithInitialTools(
    boundaryIndex?: number,
    keepInMemory: number = 2,
  ): Promise<number> {
    // Start a new batch (with checkpoint support if storage is configured)
    const newBatch = await this.startNewBatchWithAutoCheckpoint(boundaryIndex, keepInMemory);

    // Check if a tool description message already exists
    if (!this.hasToolDescriptionMessage()) {
      // Add an initial tool description message
      const toolDescMessage = this.getInitialToolDescriptionMessage();
      if (toolDescMessage) {
        this.addMessage(toolDescMessage);
      }
    }

    return newBatch;
  }

  /**
   * Execute the message operation and trigger the completion event.
   * Add tool visibility management specific to the workflow.
   * @param operation Message operation configuration
   * @returns Operation result
   */
  override async executeMessageOperation(
    operation: MessageOperationConfig,
    onAfterOperation?: (result: MessageOperationResult) => Promise<void>,
  ): Promise<MessageOperationResult> {
    const result = await super.executeMessageOperation(operation, onAfterOperation);

    // If the operation may involve changes to the visible range (such as truncation or clearing), try to re-add the tool description.
    if (operation.operation === "TRUNCATE" || operation.operation === "CLEAR") {
      if (!this.hasToolDescriptionMessage()) {
        const toolDescMessage = this.getInitialToolDescriptionMessage();
        if (toolDescMessage) {
          this.addMessage(toolDescMessage);
        }
      }
    }

    return result;
  }
}

// Re-export MessageHistoryState for convenience
export type { MessageHistoryState } from "../../../core/messaging/message-history.js";
