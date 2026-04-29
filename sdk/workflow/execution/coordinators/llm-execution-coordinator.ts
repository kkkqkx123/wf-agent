/**
 * LLM Execution Coordinator
 * Responsible for coordinating the entire process of LLM calls and tool calls
 *
 * Key Responsibilities:
 * 1. Serve as the high-level coordination entry point
 * 2. Delegate specific logic to dedicated components
 * 3. Return the final execution results
 *
 * Design Principles:
 * - Simplified coordination logic
 * - Dependency injection: Manage dependencies through LLMContextFactory
 * - Separation of responsibilities: Delegate specific execution logic to specialized components
 */

import type { LLMMessage, ID, Tool, BaseEvent, ToolSchema, LLMUsage } from "@wf-agent/types";
import { MessageRole } from "@wf-agent/types";
import type { WorkflowConfig } from "@wf-agent/types";
import { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { ToolContextStore } from "../../stores/tool-context-store.js";
import { safeEmit } from "../utils/index.js";
import type { ToolApprovalData } from "@wf-agent/types";
import { generateId } from "../../../utils/index.js";
import { getErrorOrNew } from "@wf-agent/common-utils";
import { ExecutionError } from "@wf-agent/types";
import { CheckpointCoordinator } from "../../checkpoint/checkpoint-coordinator.js";
import { InterruptionDetectorImpl, type InterruptionDetector } from "../interruption-detector.js";
import {
  checkInterruption,
  shouldContinue,
  getInterruptionDescription,
} from "@wf-agent/common-utils";
import type { InterruptionCheckResult } from "@wf-agent/common-utils";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  buildMessageAddedEvent,
  buildTokenUsageWarningEvent,
  buildConversationStateChangedEvent,
  buildUserInteractionRequestedEvent,
  buildUserInteractionProcessedEvent,
} from "../utils/event/index.js";
import {
  LLMContextFactory,
  type LLMContextFactoryConfig,
} from "../factories/llm-context-factory.js";
import { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import { prepareToolSchemasFromTools } from "../../../core/utils/tools/tool-schema-helper.js";

const logger = createContextualLogger();

/**
 * LLM Execution Parameters
 */
export interface LLMExecutionParams {
  /** Thread ID */
  threadId: string;
  /** Node ID */
  nodeId: string;
  /** Prompt words */
  prompt: string;
  /** LLM Configuration ID */
  profileId?: string;
  /** LLM parameters */
  parameters?: Record<string, unknown>;
  /** Tool List */
  tools?: unknown[];
  /** The maximum number of tool calls returned per single LLM invocation (default is 3). */
  maxToolCallsPerRequest?: number;
  /** Workflow configuration (for tool approval) */
  workflowConfig?: WorkflowConfig;
}

/**
 * The LLM execution returns results.
 */
export interface LLMExecutionResponse {
  /** Whether it was successful */
  success: boolean;
  /** LLM response content */
  content?: string;
  /** Error message */
  error?: Error;
  /** Message History */
  messages?: LLMMessage[];
}

/**
 * LLM Execution Coordinator Class
 *
 * Responsibilities:
 * - Serve as the high-level coordination entry point
 * - Directly coordinate LLM calls, tool calls, and dialogue state management
 * - Return the final execution results
 *
 * Design Principles:
 * - Simplified coordination logic
 * - Separation of responsibilities: Each component is responsible only for its own tasks
 * - Use LLMContextFactory to manage dependencies
 */
export class LLMExecutionCoordinator {
  /** Context Factory */
  private contextFactory: LLMContextFactory;

  /** Interrupt Detector (Delayed Initialization) */
  private interruptionDetector?: InterruptionDetector;

  /**
   * Constructor (using factory configuration)
   *
   * @param config Factory configuration
   */
  constructor(config: LLMContextFactoryConfig) {
    // Create a context factory
    this.contextFactory = new LLMContextFactory(config);

    // Delayed initialization of the interrupt detector
    if (config.interruptionDetector) {
      this.interruptionDetector = config.interruptionDetector;
    } else if (config.threadRegistry) {
      this.interruptionDetector = new InterruptionDetectorImpl(config.threadRegistry);
    }
  }

  /**
   * Obtain the context factory (for external access to dependencies)
   */
  getContextFactory(): LLMContextFactory {
    return this.contextFactory;
  }

  /**
   * Check if it has been aborted
   *
   * @param threadId Thread ID
   * @returns Whether it has been aborted
   */
  isAborted(threadId: string): boolean {
    if (this.interruptionDetector) {
      return this.interruptionDetector.isAborted(threadId);
    }

    // Backward compatibility: If the interruptionDetector is not provided, use the old method.
    const threadRegistry = this.contextFactory.getThreadRegistry();
    if (!threadRegistry) {
      return false;
    }

    const threadEntity = threadRegistry.get(threadId);
    if (!threadEntity) {
      return false;
    }

    return threadEntity.getAbortSignal().aborted;
  }

  /**
   * Execute LLM Call
   *
   * This method serves as a high-level coordination entry point that directly manages the processing of the entire workflow across various components:
   * 1. Manages the conversation state (through ConversationSession)
   * 2. Executes LLM calls (through LLMExecutor)
   * 3. Executes tool calls (through ToolCallExecutor)
   * 4. Triggers relevant events (through event utility functions)
   * 5. Returns the final execution results
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns Execution results
   */
  async executeLLM(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<LLMExecutionResponse> {
    // Execute the complete LLM tool invocation loop.
    const result = await this.executeLLMLoop(params, conversationState);

    // Check if it is in an interrupted state.
    if (typeof result !== "string") {
      // It is in an interrupted state.
      return {
        success: false,
        error: new Error(getInterruptionDescription(result)),
      };
    }

    // Return normally
    return {
      success: true,
      content: result,
      messages: conversationState.getMessages(),
    };
  }

  /**
   * Execute the complete LLM tool invocation cycle
   *
   * Key responsibilities:
   * 1. Execute the complete LLM tool invocation cycle.
   * 2. Control the number of iterations in the cycle.
   * 3. Manage the monitoring of Token usage.
   * 4. Handle the conversation state.
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns LLM response content or interruption status
   */
  private async executeLLMLoop(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<string | InterruptionCheckResult> {
    const { prompt, profileId, parameters, tools, maxToolCallsPerRequest, threadId, nodeId } =
      params;

    // Get the AbortSignal
    const threadRegistry = this.contextFactory.getThreadRegistry();
    const threadEntity = threadRegistry?.get(threadId);
    const abortSignal = threadEntity?.getAbortSignal();

    // Use the return value tagging system to check for interrupts.
    if (abortSignal) {
      const interruption = checkInterruption(abortSignal);
      if (!shouldContinue(interruption)) {
        return interruption;
      }
    }

    // Step 1: Add user messages
    const userMessage = {
      role: "user" as MessageRole,
      content: prompt,
    };
    conversationState.addMessage(userMessage);

    // Trigger message addition event
    const userMessageEvent = buildMessageAddedEvent({
      threadId: threadId || "",
      role: userMessage.role,
      content: userMessage.content,
      nodeId,
    });
    await safeEmit(this.contextFactory.getEventManager(), userMessageEvent);

    // Check the usage of Tokens.
    await conversationState.checkTokenUsage();

    // Check for warnings regarding the use of tokens.
    const tokenUsage = conversationState.getTokenUsage();
    if (tokenUsage) {
      const tokenLimit = 100000; // Use fixed limits or obtain them from the configuration.
      const usagePercentage = (tokenUsage.totalTokens / tokenLimit) * 100;

      // A warning is triggered when the usage exceeds 80%.
      if (usagePercentage > 80) {
        const warningEvent = buildTokenUsageWarningEvent({
          threadId: threadId || "",
          tokensUsed: tokenUsage.totalTokens,
          tokenLimit,
          usagePercentage,
        });
        await safeEmit(this.contextFactory.getEventManager(), warningEvent);
      }
    }

    // Get available tools from the tool context manager.
    let availableToolSchemas = tools;
    const toolContextStore = this.contextFactory.getToolContextStore() as
      | ToolContextStore
      | undefined;
    if (toolContextStore) {
      const availableToolIds = toolContextStore.getTools(threadId);

      if (availableToolIds.size > 0) {
        const toolService = this.contextFactory.getToolService();
        const availableTools = Array.from(availableToolIds as Set<string>)
          .map(id => {
            try {
              return toolService.getTool(id);
            } catch {
              return null;
            }
          })
          .filter(Boolean);

        // Use shared helper to convert to ToolSchema format
        availableToolSchemas = prepareToolSchemasFromTools(availableTools as Tool[]);
      }
    }

    // Check for interrupts again before executing the LLM call.
    if (abortSignal) {
      const interruption = checkInterruption(abortSignal);
      if (!shouldContinue(interruption)) {
        return interruption;
      }
    }

    // Execute an LLM call (passing the AbortSignal)
    const llmResult = await this.contextFactory.getLLMExecutor().executeLLMCall(
      conversationState.getMessages(),
      {
        prompt,
        profileId: profileId || "DEFAULT",
        parameters: parameters || {},
        tools: availableToolSchemas as ToolSchema[],
      },
      { abortSignal },
    );

    // Check if it is in an interrupted state.
    if (!llmResult.success) {
      return (llmResult as { success: false; interruption: InterruptionCheckResult }).interruption;
    }

    const result = llmResult.result;

    // Update Token usage statistics
    if (result.usage) {
      conversationState.updateTokenUsage(result.usage as LLMUsage);
    }

    // Complete the Token statistics for the current request.
    conversationState.finalizeCurrentRequest();

    // Add the LLM response to the conversation history.
    const assistantMessage = {
      role: "assistant" as MessageRole,
      content: result.content,
      toolCalls: result.toolCalls?.map((tc: { id: string; name: string; arguments: string }) => ({
        id: tc.id,
        type: "function" as const,
        function: {
          name: tc.name,
          arguments: tc.arguments,
        },
      })),
    };
    conversationState.addMessage(assistantMessage);

    // Trigger message addition event
    const assistantMessageEvent = buildMessageAddedEvent({
      threadId: threadId || "",
      role: assistantMessage.role,
      content: assistantMessage.content,
      nodeId,
    });
    await safeEmit(this.contextFactory.getEventManager(), assistantMessageEvent);

    // Check if there are any tool calls.
    if (result.toolCalls && result.toolCalls.length > 0) {
      // Verify the number of tool calls returned in a single instance.
      const maxToolsPerResponse = maxToolCallsPerRequest ?? 3;
      if (result.toolCalls.length > maxToolsPerResponse) {
        throw new ExecutionError(
          `LLM returned ${result.toolCalls.length} tool calls, ` +
            `exceeds limit of ${maxToolsPerResponse}. ` +
            `Configure maxToolCallsPerRequest to adjust this limit.`,
          nodeId,
        );
      }

      // Check for interruptions before executing the tool call.
      if (abortSignal) {
        const interruption = checkInterruption(abortSignal);
        if (!shouldContinue(interruption)) {
          return interruption;
        }
      }

      // Use the injection tool to call the executor to perform the tool call (passing the AbortSignal).
      await this.executeToolCallsWithApproval(
        result.toolCalls,
        conversationState,
        threadId,
        nodeId,
        params.workflowConfig,
        this.contextFactory.getToolCallExecutor(),
        { abortSignal },
      );
    }

    // Trigger a dialog state change event
    const finalTokenUsage = conversationState.getTokenUsage();
    const stateChangedEvent = buildConversationStateChangedEvent({
      threadId: threadId || "",
      messageCount: conversationState.getMessages().length,
      tokenUsage: finalTokenUsage?.totalTokens || 0,
      nodeId,
    });
    await safeEmit(this.contextFactory.getEventManager(), stateChangedEvent);

    // Return the final content
    return result.content;
  }

  /**
   * Execute tool calls (with approval support)
   *
   * @param toolCalls Array of tool calls
   * @param conversationState Conversation manager
   * @param threadId Thread ID
   * @param nodeId Node ID
   * @param workflowConfig Workflow configuration
   * @param toolCallExecutor Tool call executor
   * @param options Execution options (including AbortSignal)
   */
  private async executeToolCallsWithApproval(
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
    conversationState: ConversationSession,
    threadId: string,
    nodeId: string,
    workflowConfig: WorkflowConfig | undefined,
    toolCallExecutor: ToolCallExecutor,
    options?: { abortSignal?: AbortSignal },
  ): Promise<void> {
    for (const toolCall of toolCalls) {
      // Check if manual approval is required.
      if (this.requiresHumanApproval(toolCall.name, workflowConfig)) {
        const approvalResult = await this.requestToolApproval(
          toolCall,
          undefined,
          threadId,
          nodeId,
        );

        if (!approvalResult.approved) {
          // User refused; skip this tool call.
          const toolMessage = {
            role: "tool" as MessageRole,
            content: JSON.stringify({
              error: "Tool call was rejected by user approval",
              rejected: true,
            }),
            toolCallId: toolCall.id,
          };
          conversationState.addMessage(toolMessage);
          continue;
        }

        // If the user provides the edited parameters
        if (approvalResult.editedParameters) {
          toolCall.arguments = JSON.stringify(approvalResult.editedParameters);
        }

        // If the user provides additional instructions, add them to the conversation history.
        if (approvalResult.userInstruction) {
          conversationState.addMessage({
            role: "user" as MessageRole,
            content: approvalResult.userInstruction,
          });
        }
      }

      // Execute tool invocation (passing the AbortSignal)
      await toolCallExecutor.executeToolCalls(
        [toolCall],
        conversationState,
        threadId,
        nodeId,
        options,
      );
    }
  }

  /**
   * Check if the tool requires manual approval
   *
   * @param toolId Tool ID
   * @param workflowConfig Workflow configuration
   * @returns Whether approval is required
   */
  private requiresHumanApproval(toolId: ID, workflowConfig: WorkflowConfig | undefined): boolean {
    // If no approval is configured, then no approval is required.
    if (!workflowConfig?.toolApproval) {
      return false;
    }

    const autoApproved = workflowConfig.toolApproval.autoApprovedTools || [];
    return !autoApproved.includes(toolId);
  }

  /**
   * Request tool approval
   *
   * @param toolCall: Tool call
   * @param approvalConfig: Approval configuration
   * @param threadId: Thread ID
   * @param nodeId: Node ID
   * @param conversationState: Conversation manager
   * @returns: Approval result
   */
  private async requestToolApproval(
    toolCall: { id: string; name: string; arguments: string },
    approvalConfig: { approvalTimeout?: number } | undefined,
    threadId: string,
    nodeId: string,
  ): Promise<ToolApprovalData> {
    const interactionId = generateId();
    const tool = this.contextFactory.getToolService().getTool(toolCall.id);

    // If there is an execution context, create checkpoints to support long-term approval processes.
    let checkpointId: string | undefined;
    if (this.contextFactory.hasToolApprovalSupport()) {
      try {
        const approvalContext = this.contextFactory.createToolApprovalContext(threadId, nodeId);
        if (approvalContext.workflowRegistry && approvalContext.graphRegistry && approvalContext.threadRegistry) {
          const dependencies = {
            workflowExecutionRegistry: approvalContext.threadRegistry,
            checkpointStateManager: approvalContext.checkpointStateManager,
            workflowRegistry: approvalContext.workflowRegistry,
            workflowGraphRegistry: approvalContext.graphRegistry,
          };
          checkpointId = await CheckpointCoordinator.createCheckpoint(threadId, dependencies, {
            description: "Waiting for tool approval",
            customFields: {
              toolApprovalState: {
                pendingToolCall: toolCall,
                interactionId,
              },
            },
          });
        }
      } catch (error) {
        // Record warning logs without interrupting the execution.
        logger.warn(
          "Failed to create checkpoint for tool approval",
          {
            operation: "create_checkpoint",
            toolCallId: toolCall.id,
            threadId,
            nodeId,
            suggestion: "Check checkpoint storage configuration and retry",
          },
          undefined,
          getErrorOrNew(error),
        );
      }
    }

    try {
      // Trigger the USER_INTERACTION_REQUESTED event
      const requestedEvent = buildUserInteractionRequestedEvent({
        threadId,
        nodeId,
        interactionId,
        operationType: "TOOL_APPROVAL",
        prompt: `Do you approve calling the tool "${toolCall.id}"?`,
        timeout: approvalConfig?.approvalTimeout || 0,
      });
      await safeEmit(this.contextFactory.getEventManager(), requestedEvent);

      // Wait for the USER_INTERACTION_RESPONDED event to be triggered.
      const response = await this.waitForUserInteractionResponse(
        interactionId,
        approvalConfig?.approvalTimeout || 0,
      );

      // Parse approval results
      const approvalResult = response.inputData as ToolApprovalData;

      // Trigger the USER_INTERACTION_PROCESSED event
      const processedEvent = buildUserInteractionProcessedEvent({
        threadId,
        interactionId,
        operationType: "TOOL_APPROVAL",
        results: approvalResult,
      });
      await safeEmit(this.contextFactory.getEventManager(), processedEvent);

      return approvalResult;
    } finally {
      // Clean up the checkpoints (if any exist).
      const checkpointStateManager = this.contextFactory.getCheckpointStateManager();
      if (checkpointId && checkpointStateManager) {
        try {
          await checkpointStateManager.delete(checkpointId);
        } catch (error) {
          // Record warning logs without interrupting the execution.
          logger.warn(
            "Failed to cleanup checkpoint",
            {
              operation: "cleanup_checkpoint",
              checkpointId,
              threadId,
              nodeId,
              suggestion:
                "Checkpoint cleanup failed, may leave stale data. Check storage permissions and retry",
            },
            undefined,
            getErrorOrNew(error),
          );
        }
      }
    }
  }

  /**
   * Waiting for user interaction response
   *
   * @param interactionId: Interaction ID
   * @param timeoutMs: Timeout period in milliseconds; 0 indicates infinite waiting
   * @returns: User response event
   */
  private waitForUserInteractionResponse(
    interactionId: string,
    timeoutMs: number = 0,
  ): Promise<{ interactionId: string; inputData: ToolApprovalData }> {
    return new Promise((resolve, reject) => {
      let timeoutId: NodeJS.Timeout | null = null;
      const eventManager = this.contextFactory.getEventManager();

      const handler = (event: BaseEvent) => {
        const typedEvent = event as unknown as {
          interactionId: string;
          inputData: ToolApprovalData;
        };
        if (typedEvent.interactionId === interactionId) {
          if (timeoutId) {
            clearTimeout(timeoutId);
          }
          eventManager.off("USER_INTERACTION_RESPONDED", handler);
          resolve(typedEvent);
        }
      };

      // The timeout is set only when timeoutMs > 0.
      if (timeoutMs > 0) {
        timeoutId = setTimeout(() => {
          eventManager.off("USER_INTERACTION_RESPONDED", handler);
          reject(new Error(`User interaction timeout after ${timeoutMs}ms`));
        }, timeoutMs);
      }

      eventManager.on("USER_INTERACTION_RESPONDED", handler);
    });
  }
}
