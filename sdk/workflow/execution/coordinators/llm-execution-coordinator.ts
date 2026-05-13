/**
 * LLM Execution Coordinator (Workflow-Specific)
 * Coordinates LLM execution with workflow-specific features
 *
 * Key Responsibilities:
 * 1. Compose core LLMExecutionCoordinator for shared LLM logic
 * 2. Add workflow-specific features: tool approval, checkpoint tracking, operation state
 * 3. Manage workflow context through LLMContextFactory
 *
 * Design Principles:
 * - Composition over inheritance: delegates core LLM logic to shared coordinator
 * - Separation of concerns: workflow-specific logic isolated from core coordination
 * - Dependency injection: Manage dependencies through LLMContextFactory
 */

import type { LLMMessage, BaseEvent, LLMToolCall } from "@wf-agent/types";
import type { WorkflowConfig } from "@wf-agent/types";
import { ConversationSession } from "../../../core/messaging/conversation-session.js";
import type { ToolContextStore } from "../../stores/tool-context-store.js";
import { emit } from "../utils/index.js";
import type { ToolApprovalResult } from "@wf-agent/types";
import { generateId } from "../../../utils/index.js";
import { getErrorOrNew, now } from "@wf-agent/common-utils";
import { ExecutionError } from "@wf-agent/types";
import { CheckpointCoordinator } from "../../checkpoint/checkpoint-coordinator.js";
import { InterruptionDetectorImpl, type InterruptionDetector } from "../interruption-detector.js";
import {
  checkWorkflowInterruption,
  shouldContinue,
  getWorkflowInterruptionDescription,
} from "../../../core/utils/interruption/index.js";
import type { WorkflowInterruptionCheckResult } from "../../../core/utils/interruption/index.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import {
  buildToolApprovalRequestedEvent,
} from "../utils/event/index.js";
import {
  LLMContextFactory,
  type LLMContextFactoryConfig,
} from "../factories/llm-context-factory.js";
import { ToolCallExecutor } from "../../../core/executors/tool-call-executor.js";
import { ToolApprovalCoordinator } from "../../../core/coordinators/tool-approval-coordinator.js";
import { LLMExecutionCoordinator as CoreLLMExecutionCoordinator } from "../../../core/coordinators/llm-execution-coordinator.js";

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
 * LLM Execution Coordinator Class (Workflow-Specific)
 *
 * Responsibilities:
 * - Compose core LLMExecutionCoordinator for shared LLM + tool execution logic
 * - Add workflow-specific features: tool approval, checkpoint management, operation tracking
 * - Return the final execution results
 *
 * Design Principles:
 * - Composition pattern: delegates core LLM logic to shared coordinator
 * - Workflow extensions: adds approval, checkpoints, and operation state on top
 * - Use LLMContextFactory to manage dependencies
 */
export class LLMExecutionCoordinator {
  /** Context Factory */
  private contextFactory: LLMContextFactory;

  /** Interrupt Detector (Delayed Initialization) */
  private interruptionDetector?: InterruptionDetector;

  /** Tool Approval Coordinator */
  private approvalCoordinator: ToolApprovalCoordinator;

  /** Core LLM Coordinator (composed for shared LLM logic) */
  private coreCoordinator: CoreLLMExecutionCoordinator;

  /**
   * Constructor (using factory configuration)
   *
   * @param config Factory configuration
   */
  constructor(config: LLMContextFactoryConfig) {
    // Create a context factory
    this.contextFactory = new LLMContextFactory(config);

    // Initialize core coordinator with shared LLM logic
    this.coreCoordinator = new CoreLLMExecutionCoordinator(
      config.llmExecutor,
      config.toolCallExecutor,
    );

    // Delayed initialization of the interrupt detector
    if (config.interruptionDetector) {
      this.interruptionDetector = config.interruptionDetector;
    } else if (config.executionRegistry) {
      this.interruptionDetector = new InterruptionDetectorImpl(config.executionRegistry);
    }

    // Initialize approval coordinator
    this.approvalCoordinator = new ToolApprovalCoordinator(config.eventManager);
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
    if (!this.interruptionDetector) {
      logger.warn("InterruptionDetector not initialized, cannot check abort status", {
        executionId,
      });
      return false;
    }

    return this.interruptionDetector.isAborted(executionId);
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
    // Execute single LLM call with optional tool execution
    const result = await this.executeSingleLLMCall(params, conversationState);

    // Check if it is in an interrupted state.
    if (typeof result !== "string") {
      // It is in an interrupted state.
      return {
        success: false,
        error: new Error(getWorkflowInterruptionDescription(result)),
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
   * Execute a single LLM call with workflow-specific tool approval
   *
   * Key responsibilities:
   * 1. Delegate LLM call to shared coordinator (message management, token tracking)
   * 2. Add workflow-specific tool approval before executing tools
   * 3. Handle operation state tracking for checkpoint/resume
   *
   * Note: This executes ONE LLM call, not a loop.
   * Workflow iteration logic is handled by the node handler.
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns LLM response content or interruption status
   */
  private async executeSingleLLMCall(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<string | WorkflowInterruptionCheckResult> {
    const { prompt, profileId, parameters, tools, maxToolCallsPerRequest, executionId, nodeId } =
      params;

    // Get the AbortSignal
    const executionRegistry = this.contextFactory.getWorkflowExecutionRegistry();
    const executionEntity = executionRegistry?.get(executionId);
    const abortSignal = executionEntity?.getAbortSignal();

    // Check interruption before starting
    if (abortSignal) {
      const interruption = checkWorkflowInterruption(abortSignal);
      if (!shouldContinue(interruption)) {
        return interruption;
      }
    }

    // Workflow-specific: Track operation state for mid-node resume
    if (executionEntity) {
      const operationId = generateId();
      executionEntity.state.setCurrentOperation({
        type: "LLM_STREAMING",
        operationId,
        nodeId,
        startedAt: now(),
        progress: {
          tokensGenerated: 0,
        },
        metadata: {
          profileId: profileId || "DEFAULT",
        },
      });
    }

    try {
      // Workflow-specific: Prepare tools from tool context store
      let availableTools = tools;
      const toolContextStore = this.contextFactory.getToolContextStore() as
        | ToolContextStore
        | undefined;
      if (toolContextStore) {
        const availableToolIds = toolContextStore.getTools(executionId);

        if (availableToolIds.size > 0) {
          const toolService = this.contextFactory.getToolService();
          const resolvedTools = Array.from(availableToolIds as Set<string>)
            .map(id => {
              try {
                return toolService.getTool(id);
              } catch {
                return null;
              }
            })
            .filter(Boolean);

          availableTools = resolvedTools;
        }
      }

      // Build config for core coordinator
      const llmConfig = {
        profileId: profileId || "DEFAULT",
        parameters: parameters || {},
        maxToolCallsPerRequest,
        enableTokenTracking: true,
        tokenWarningThreshold: 80,
        tokenLimit: 100000,
      };

      // Call core coordinator to execute LLM (without tool execution)
      // Core handles: message management, token tracking, events, LLM call
      const coreResult = await this.coreCoordinator.executeLLM(
        {
          contextId: executionId,
          prompt,
          config: llmConfig,
          tools: availableTools,
          abortSignal,
          eventManager: this.contextFactory.getEventManager(),
          nodeId,
          executeTools: false,  // Don't execute tools, we'll handle them with approval
        },
        conversationState,
      );;

      // Check if core execution succeeded
      if (!coreResult.success) {
        // If interrupted, preserve operation state for resume
        if (executionEntity && abortSignal?.aborted) {
          logger.info("LLM call interrupted, preserving operation state for resume", {
            executionId,
            nodeId,
          });
        } else {
          // On other errors, clear operation state
          executionEntity?.state.clearOperation();
        }
        
        // Convert error to interruption result
        const isPaused = coreResult.error?.message?.includes("paused") || false;
        return {
          type: isPaused ? "paused" : "stopped",
          nodeId,
          executionId,
        };
      }

      // Get the latest assistant message to check for tool calls
      const messages = conversationState.getMessages();
      const lastMessage = messages[messages.length - 1];
      
      if (lastMessage && lastMessage.toolCalls && lastMessage.toolCalls.length > 0) {
        // Validate single response tool call count
        const maxToolsPerResponse = maxToolCallsPerRequest ?? 3;
        if (lastMessage.toolCalls.length > maxToolsPerResponse) {
          throw new ExecutionError(
            `LLM returned ${lastMessage.toolCalls.length} tool calls, ` +
              `exceeds limit of ${maxToolsPerResponse}. ` +
              `Configure maxToolCallsPerRequest to adjust this limit.`,
            nodeId,
          );
        }

        // Check for interruptions before executing the tool call.
        if (abortSignal) {
          const interruption = checkWorkflowInterruption(abortSignal);
          if (!shouldContinue(interruption)) {
            return interruption;
          }
        }

        // Execute tool calls with approval
        await this.executeToolCallsWithApproval(
          lastMessage.toolCalls.map(tc => ({
            id: tc.id,
            name: tc.function.name,
            arguments: tc.function.arguments,
          })),
          conversationState,
          executionId,
          nodeId,
          params.workflowConfig,
          this.contextFactory.getToolCallExecutor(),
          { abortSignal },
        );
      }

      // Clear operation state on success
      executionEntity?.state.clearOperation();

      // Return final content
      return coreResult.content || "";
    } catch (error) {
      // On exception, clear operation state
      executionEntity?.state.clearOperation();
      throw error;
    }
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
    // Convert to LLMToolCall format for batch processing
    const llmToolCalls = toolCalls.map(tc => ({
      id: tc.id,
      type: "function" as const,
      function: {
        name: tc.name,
        arguments: tc.arguments,
      },
    }));

    // Process batch through approval coordinator
    const batchResult = await this.approvalCoordinator.processToolBatch(
      llmToolCalls,
      workflowConfig?.toolApproval || {},
      executionId,
      nodeId,
      {
        requestApproval: async (request) => {
          return this.requestToolApprovalInternal(request, executionId, nodeId);
        },
      },
      this.contextFactory.getEventManager(),
    );

    // Execute auto-approved tools
    for (let i = 0; i < batchResult.autoExecuted.length; i++) {
      const autoResult = batchResult.autoExecuted[i];
      // Auto-executed tools correspond to the first N tools in the original list
      if (i < toolCalls.length && autoResult) {
        const originalToolCall = toolCalls[i]!;
        await toolCallExecutor.executeToolCalls(
          [originalToolCall],
          conversationState,
          executionId,
          nodeId,
          options,
        );
      }
    }

    // Handle confirmation-required tool
    if (batchResult.confirmationRequired && batchResult.confirmationResult?.approved) {
      const confirmedToolCall = toolCalls.find(
        tc => tc.id === batchResult.confirmationRequired?.id,
      );
      if (confirmedToolCall) {
        // Apply edited parameters if provided
        if (batchResult.confirmationResult.editedParameters) {
          confirmedToolCall.arguments = JSON.stringify(
            batchResult.confirmationResult.editedParameters,
          );
        }

        // Add user instruction if provided
        if (batchResult.confirmationResult.userInstruction) {
          conversationState.addMessage({
            role: "user" as const,
            content: batchResult.confirmationResult.userInstruction,
          });
        }

        await toolCallExecutor.executeToolCalls(
          [confirmedToolCall],
          conversationState,
          executionId,
          nodeId,
          options,
        );
      }

      // Continue with remaining queue if flag is set
      if (
        batchResult.confirmationResult.continueBatch !== false &&
        batchResult.remainingQueue.length > 0
      ) {
        const remainingToolCalls = batchResult.remainingQueue.map(tc => ({
          id: tc.id,
          name: tc.function?.name || "unknown",
          arguments: tc.function?.arguments || "{}",
        }));

        // Recursively process remaining tools
        await this.executeToolCallsWithApproval(
          remainingToolCalls,
          conversationState,
          executionId,
          nodeId,
          workflowConfig,
          toolCallExecutor,
          options,
        );
      }
    }
  }

  /**
   * Request tool approval
   *
   * @param toolCall: Tool call
   * @param approvalConfig: Approval configuration
   * @param executionId: Execution ID
   * @param nodeId: StaticNode ID
   * @param conversationState: Conversation manager
   * @returns: Approval result
   */
  private async requestToolApproval(
    toolCall: LLMToolCall,
    approvalConfig: { approvalTimeout?: number } | undefined,
    executionId: string,
    nodeId: string,
  ): Promise<ToolApprovalResult> {
    const interactionId = generateId();

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
      // Trigger the TOOL_APPROVAL_REQUESTED event
      const requestedEvent = buildToolApprovalRequestedEvent({
        executionId,
        nodeId,
        interactionId,
        toolCall,
        contextId: executionId,
        timeout: approvalConfig?.approvalTimeout || 0,
      });
      
      try {
        await emit(this.contextFactory.getEventManager(), requestedEvent);
      } catch (error) {
        logger.debug("Failed to emit TOOL_APPROVAL_REQUESTED event", { eventType: requestedEvent.type, error });
      }

      // Wait for the USER_INTERACTION_RESPONDED event to be triggered.
      const response = await this.waitForUserInteractionResponse(
        interactionId,
        approvalConfig?.approvalTimeout || 0,
      );

      // Parse approval results
      const approvalResult = response.inputData as ToolApprovalResult;

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
   * Internal method to request tool approval (adapts ToolApprovalCoordinator interface)
   *
   * @param request - Approval request from ToolApprovalCoordinator
   * @param executionId - Execution ID
   * @param nodeId - Node ID
   * @returns Approval result in new ToolApprovalResult format
   */
  private async requestToolApprovalInternal(
    request: { toolCall: { id: string; function?: { name?: string; arguments?: string } } },
    executionId: string,
    nodeId: string,
  ): Promise<{ approved: boolean; toolCallId: string; editedParameters?: Record<string, unknown>; userInstruction?: string; rejectionReason?: string }> {
    const toolCallName = request.toolCall.function?.name || "";
    const toolCallArgs = request.toolCall.function?.arguments || "{}";

    // Call the approval flow which returns ToolApprovalResult
    const result = await this.requestToolApproval(
      request.toolCall as import("@wf-agent/types").LLMToolCall,
      undefined,
      executionId,
      nodeId,
    );

    // Return in the format expected by ToolApprovalCoordinator
    return {
      approved: result.approved,
      toolCallId: request.toolCall.id,
      editedParameters: result.editedParameters,
      userInstruction: result.userInstruction,
      rejectionReason: !result.approved ? "Tool call was rejected by user" : undefined,
    };
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
  ): Promise<{ interactionId: string; inputData: ToolApprovalResult }> {
    return new Promise((resolve, reject) => {
      let timeoutId: NodeJS.Timeout | null = null;
      const eventManager = this.contextFactory.getEventManager();

      const handler = (event: BaseEvent) => {
        const typedEvent = event as unknown as {
          interactionId: string;
          inputData: ToolApprovalResult;
        };
        if (typedEvent.interactionId === interactionId) {
          if (timeoutId) {
            clearTimeout(timeoutId);
          }
          eventManager.off("TOOL_APPROVAL_RESPONDED", handler);
          resolve(typedEvent);
        }
      };

      // The timeout is set only when timeoutMs > 0.
      if (timeoutMs > 0) {
        timeoutId = setTimeout(() => {
          eventManager.off("TOOL_APPROVAL_RESPONDED", handler);
          reject(new Error(`User interaction timeout after ${timeoutMs}ms`));
        }, timeoutMs);
      }

      eventManager.on("TOOL_APPROVAL_RESPONDED", handler);
    });
  }
}

