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
import { emit } from "../utils/index.js";
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
  /** Execution ID */
  executionId: string;
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
    } else if (config.executionRegistry) {
      this.interruptionDetector = new InterruptionDetectorImpl(config.executionRegistry);
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
   * @param executionId Execution ID
   * @returns Whether it has been aborted
   */
  isAborted(executionId: string): boolean {
    if (this.interruptionDetector) {
      return this.interruptionDetector.isAborted(executionId);
    }

    // Backward compatibility: If the interruptionDetector is not provided, use the old method.
    const executionRegistry = this.contextFactory.getWorkflowExecutionRegistry();
    if (!executionRegistry) {
      return false;
    }

    const executionEntity = executionRegistry.get(executionId);
    if (!executionEntity) {
      return false;
    }

    return executionEntity.getAbortSignal().aborted;
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
    const { prompt, profileId, parameters, tools, maxToolCallsPerRequest, executionId, nodeId } =
      params;

    // Get the AbortSignal
    const executionRegistry = this.contextFactory.getWorkflowExecutionRegistry();
    const executionEntity = executionRegistry?.get(executionId);
    const abortSignal = executionEntity?.getAbortSignal();

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
      executionId: executionId || "",
      role: userMessage.role,
      content: userMessage.content,
      nodeId,
    });
    
    try {
      await emit(this.contextFactory.getEventManager(), userMessageEvent);
    } catch (error) {
      logger.debug("Failed to emit MESSAGE_ADDED event", { eventType: userMessageEvent.type, error });
    }

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
          executionId: executionId || "",
          tokensUsed: tokenUsage.totalTokens,
          tokenLimit,
          usagePercentage,
        });
        
        try {
          await emit(this.contextFactory.getEventManager(), warningEvent);
        } catch (error) {
          logger.debug("Failed to emit TOKEN_USAGE_WARNING event", { eventType: warningEvent.type, error });
        }
      }
    }

    // Get available tools from the tool context manager.
    let availableToolSchemas = tools;
    const toolContextStore = this.contextFactory.getToolContextStore() as
      | ToolContextStore
      | undefined;
    if (toolContextStore) {
      const availableToolIds = toolContextStore.getTools(executionId);

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
      executionId: executionId || "",
      role: assistantMessage.role,
      content: assistantMessage.content,
      nodeId,
    });
    
    try {
      await emit(this.contextFactory.getEventManager(), assistantMessageEvent);
    } catch (error) {
      logger.debug("Failed to emit MESSAGE_ADDED event", { eventType: assistantMessageEvent.type, error });
    }

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
        executionId,
        nodeId,
        params.workflowConfig,
        this.contextFactory.getToolCallExecutor(),
        { abortSignal },
      );
    }

    // Trigger a dialog state change event
    const finalTokenUsage = conversationState.getTokenUsage();
    const stateChangedEvent = buildConversationStateChangedEvent({
      executionId: executionId || "",
      messageCount: conversationState.getMessages().length,
      tokenUsage: finalTokenUsage?.totalTokens || 0,
      nodeId,
    });
    
    try {
      await emit(this.contextFactory.getEventManager(), stateChangedEvent);
    } catch (error) {
      logger.debug("Failed to emit CONVERSATION_STATE_CHANGED event", { eventType: stateChangedEvent.type, error });
    }

    // Return the final content
    return result.content;
  }

  /**
   * Execute tool calls (with approval support)
   *
   * @param toolCalls Array of tool calls
   * @param conversationState Conversation manager
   * @param executionId Execution ID
   * @param nodeId Node ID
   * @param workflowConfig Workflow configuration
   * @param toolCallExecutor Tool call executor
   * @param options Execution options (including AbortSignal)
   */
  private async executeToolCallsWithApproval(
    toolCalls: Array<{ id: string; name: string; arguments: string }>,
    conversationState: ConversationSession,
    executionId: string,
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
          executionId,
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
        executionId,
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
   * @param executionId: Execution ID
   * @param nodeId: Node ID
   * @param conversationState: Conversation manager
   * @returns: Approval result
   */
  private async requestToolApproval(
    toolCall: { id: string; name: string; arguments: string },
    approvalConfig: { approvalTimeout?: number } | undefined,
    executionId: string,
    nodeId: string,
  ): Promise<ToolApprovalData> {
    const interactionId = generateId();
    const tool = this.contextFactory.getToolService().getTool(toolCall.id);

    // If there is an execution context, create checkpoints to support long-term approval processes.
    let checkpointId: string | undefined;
    if (this.contextFactory.hasToolApprovalSupport()) {
      try {
        const approvalContext = this.contextFactory.createToolApprovalContext(executionId, nodeId);
        if (approvalContext.workflowRegistry && approvalContext.graphRegistry && approvalContext.executionRegistry) {
          const dependencies = {
            workflowExecutionRegistry: approvalContext.executionRegistry,
            checkpointStateManager: approvalContext.checkpointStateManager,
            workflowRegistry: approvalContext.workflowRegistry,
            workflowGraphRegistry: approvalContext.graphRegistry,
          };
          checkpointId = await CheckpointCoordinator.createCheckpoint(executionId, dependencies, {
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
            executionId,
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
        executionId,
        nodeId,
        interactionId,
        operationType: "TOOL_APPROVAL",
        prompt: `Do you approve calling the tool "${toolCall.id}"?`,
        timeout: approvalConfig?.approvalTimeout || 0,
      });
      
      try {
        await emit(this.contextFactory.getEventManager(), requestedEvent);
      } catch (error) {
        logger.debug("Failed to emit USER_INTERACTION_REQUESTED event", { eventType: requestedEvent.type, error });
      }

      // Wait for the USER_INTERACTION_RESPONDED event to be triggered.
      const response = await this.waitForUserInteractionResponse(
        interactionId,
        approvalConfig?.approvalTimeout || 0,
      );

      // Parse approval results
      const approvalResult = response.inputData as ToolApprovalData;

      // Trigger the USER_INTERACTION_PROCESSED event
      const processedEvent = buildUserInteractionProcessedEvent({
        executionId,
        interactionId,
        operationType: "TOOL_APPROVAL",
        results: approvalResult,
      });
      
      try {
        await emit(this.contextFactory.getEventManager(), processedEvent);
      } catch (error) {
        logger.debug("Failed to emit USER_INTERACTION_PROCESSED event", { eventType: processedEvent.type, error });
      }

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
              executionId,
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

