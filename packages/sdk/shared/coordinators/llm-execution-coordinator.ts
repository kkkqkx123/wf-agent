/**
 * LLM Execution Coordinator
 * Coordinates LLM calls and tool calls execution flow
 *
 * Core Responsibilities:
 * 1. Coordinate LLM calls
 * 2. Coordinate tool calls
 * 3. Manage conversation state
 * 4. Monitor token usage
 * 5. Handle interruption (via AbortSignal)
 * 6. Trigger LLM-related events (message, token, conversation state)
 *
 * Design Principles:
 * - Pure LLM coordination logic
 * - Contains only LLM-call-inherent features (events, token tracking)
 * - No business-specific features (tool approval, checkpoint)
 * - Reusable across modules (Graph, Agent, etc.)
 */

import type {
  LLMMessage,
  ToolSchema,
  LLMUsage,
  TransformContextFn,
  DynamicPromptContext,
  DynamicPromptInjection,
} from "@wf-agent/types";
import type { LLMExecutionConfig } from "@wf-agent/types";
import { MessageRole } from "@wf-agent/types";
import { ConversationSession } from "../messaging/conversation-session.js";
import { ExecutionError } from "@wf-agent/types";
import {
  executeWithInterruptionHandling,
  getExecutionInterruptionDescription,
} from "../utils/interruption/index.js";
import type { ExecutionInterruptionCheckResult } from "../utils/interruption/index.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { LLMExecutor, type LLMExecutionResult } from "../../services/executors/llm-executor.js";
import { LLMWrapper } from "../../services/llm/wrapper.js";
import { ToolCallExecutor } from "../../services/executors/tool-call-executor.js";
import { prepareToolSchemasFromTools } from "../utils/tools/tool-schema-helper.js";
import type { EventRegistry } from "../registry/event-registry.js";
import type { TokenMetricsCollector } from "../../metrics/token-collector.js";
import type { MessageStream } from "../../services/llm/message-stream.js";
import {
  buildMessageAddedEvent,
  buildTokenUsageWarningEvent,
  buildConversationStateChangedEvent,
} from "../utils/event/builders/index.js";

const logger = createContextualLogger();

/**
 * LLM Execution Params
 */
export interface LLMExecutionParams {
  /** Execution context ID (execution ID, session ID, etc.) */
  contextId: string;
  /** Prompt text */
  prompt: string;
  /** LLM execution configuration */
  config: LLMExecutionConfig;
  /** Available tools */
  tools?: unknown[];
  /** Abort signal for cancellation */
  abortSignal?: AbortSignal;
  /** Event manager for triggering events */
  eventManager?: EventRegistry;
  /** Node ID (optional, for error tracking) */
  nodeId?: string;
  /** Whether to execute tool calls automatically (default: true) */
  executeTools?: boolean;
  /**
   * Transform context before LLM call
   *
   * Called to transform the message context before each LLM call.
   * Use for message compression, history pruning, or dynamic context injection.
   * When provided, the coordinator applies this transform after retrieving
   * messages from the conversation state and before passing them to the LLM executor.
   */
  transformContext?: TransformContextFn;
}

/**
 * LLM Execution Response
 */
export interface LLMExecutionResponse {
  /** Whether execution succeeded */
  success: boolean;
  /** LLM response content (if successful) */
  content?: string;
  /** Error information (if failed) */
  error?: Error;
  /** Message history */
  messages?: LLMMessage[];
}

/**
 * LLM Execution Coordinator Class
 *
 * Responsibilities:
 * - Coordinate LLM calls and tool calls
 * - Manage conversation state
 * - Monitor token usage
 * - Handle interruption
 * - Trigger LLM-related events
 *
 * Design Principles:
 * - Pure coordination logic
 * - Contains only LLM-call-inherent features
 * - Reusable across modules
 */
export class LLMExecutionCoordinator {
  /**
   * Constructor
   *
   * @param llmExecutor LLM executor
   * @param toolCallExecutor Tool call executor
   * @param tokenMetricsCollector Optional token metrics collector
   * @param llmWrapper Optional LLM wrapper (required for streaming support)
   */
  constructor(
    private llmExecutor: LLMExecutor,
    private toolCallExecutor: ToolCallExecutor,
    private tokenMetricsCollector?: TokenMetricsCollector,
    private llmWrapper?: LLMWrapper,
  ) {}

  /**
   * Execute LLM call
   *
   * This method coordinates the complete LLM-tool call loop:
   * 1. Add user message to conversation
   * 2. Execute LLM call
   * 3. Execute tool calls (if any)
   * 4. Trigger events (message, token, conversation state)
   * 5. Return final result
   *
   * Note: Tool approval and checkpoint creation should be handled by the caller.
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns Execution result
   */
  async executeLLM(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<LLMExecutionResponse> {
    try {
      // Execute single LLM call with optional tool execution
      const result = await this.executeSingleLLMCall(params, conversationState);

      // Guard against undefined/null return (should not happen in normal flow)
      if (result === undefined || result === null) {
        return {
          success: false,
          error: new Error("LLM execution returned no result"),
        };
      }

      // Check if it's an interruption state
      if (typeof result !== "string") {
        // It's an interruption state
        const description = getExecutionInterruptionDescription(result);
        const error = new Error(description);
        // Preserve original interruption info as cause if available
        if (result && typeof result === "object" && "reason" in result) {
          Object.defineProperty(error, "cause", { value: result });
        }
        return {
          success: false,
          error,
        };
      }

      // Normal return
      return {
        success: true,
        content: result,
        messages: conversationState.getMessages(),
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error : new Error(String(error)),
      };
    }
  }

  /**
   * Execute LLM stream
   *
   * Applies transformContext (if provided), then initiates a streaming LLM call
   * via LLMWrapper. Returns the MessageStream for the caller to consume events.
   *
   * Note: Unlike executeLLM(), this method does NOT:
   * - Add user messages to conversation state
   * - Execute tool calls automatically
   * - Track token usage
   * The caller is responsible for managing conversation state and tool execution
   * when using streaming mode.
   *
   * @param params Execution parameters (contextId, prompt, config, tools, abortSignal, transformContext)
   * @param conversationState Conversation manager (used only to retrieve messages)
   * @returns MessageStream for event consumption
   * @throws Error if LLMWrapper is not configured or LLM call fails
   */
  async executeLLMStream(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<MessageStream> {
    if (!this.llmWrapper) {
      throw new Error(
        "LLMWrapper is required for streaming execution. " +
          "Please provide it in the LLMExecutionCoordinator constructor.",
      );
    }

    const { config, tools, abortSignal, transformContext } = params;

    const { profileId, parameters } = config;

    // Retrieve messages from conversation state
    let messages = conversationState.getMessages();

    // Process dynamic prompt injection if provided
    let systemPromptSuffix = "";
    let userContextSuffix = "";

    if (transformContext) {
      const dynamicContext: DynamicPromptContext = {
        timestamp: Date.now(),
        messageCount: messages.length,
        executionId: params.contextId,
        signal: abortSignal,
      };

      const injection: DynamicPromptInjection = await transformContext(dynamicContext);

      if (injection.staticSystem) {
        systemPromptSuffix = injection.staticSystem;
      }
      if (injection.dynamicUserContext) {
        userContextSuffix = injection.dynamicUserContext;
      }
    }

    // Apply dynamic prompts to messages
    if (systemPromptSuffix || userContextSuffix) {
      messages = this.injectDynamicPrompts(messages, systemPromptSuffix, userContextSuffix);
    }

    // Execute streaming LLM call
    const streamResult = await this.llmWrapper.generateStream({
      profileId: profileId || "DEFAULT",
      messages,
      tools: tools as ToolSchema[],
      parameters: parameters || {},
      stream: true,
      signal: abortSignal,
    });

    if (streamResult.isErr()) {
      throw streamResult.error;
    }

    return streamResult.value;
  }

  /**
   * Execute LLM call with pre-built messages
   *
   * A lower-level method that accepts pre-built messages directly and applies
   * transformContext before calling the LLM. Unlike executeLLM(), this method:
   * - Does NOT add user messages to conversation state
   * - Does NOT execute tool calls
   * - Does NOT track token usage
   * - Does NOT trigger events
   *
   * Dynamic prompt injection (if provided):
   * - staticSystem: Merged into system message (stable, cached)
   * - dynamicUserContext: Appended to last user message (variable, not cached)
   *
   * This is designed for callers that have their own conversation management
   * and execution flow (e.g., AgentExecutionCoordinator).
   *
   * @param messages Pre-built message array (already includes all user/assistant messages)
   * @param config LLM configuration (profileId, parameters, tools)
   * @param options Execution options (abortSignal, executionId, nodeId)
   * @param transformContext Optional transform function for dynamic prompt injection
   * @returns Raw LLM execution result
   */
  async executeLLMCallWithMessages(
    messages: LLMMessage[],
    config: {
      profileId: string;
      parameters: Record<string, unknown>;
      tools?: ToolSchema[];
    },
    options: {
      abortSignal?: AbortSignal;
      executionId: string;
      nodeId?: string;
      messageCount?: number;
      currentIteration?: number;
    },
    transformContext?: TransformContextFn,
  ): Promise<LLMExecutionResult> {
    const { abortSignal, executionId, nodeId, messageCount, currentIteration } = options;

    // Process dynamic prompt injection if provided
    let llmMessages = [...messages];
    let systemPromptSuffix = "";
    let userContextSuffix = "";

    if (transformContext) {
      const dynamicContext: DynamicPromptContext = {
        timestamp: Date.now(),
        messageCount: messageCount || messages.length,
        currentIteration,
        executionId,
        signal: abortSignal,
      };

      const injection: DynamicPromptInjection = await transformContext(dynamicContext);

      if (injection.staticSystem) {
        systemPromptSuffix = injection.staticSystem;
      }
      if (injection.dynamicUserContext) {
        userContextSuffix = injection.dynamicUserContext;
      }
    }

    // Apply dynamic prompts to messages
    if (systemPromptSuffix || userContextSuffix) {
      llmMessages = this.injectDynamicPrompts(
        llmMessages,
        systemPromptSuffix,
        userContextSuffix,
      );
    }

    return await this.llmExecutor.executeLLMCall(
      llmMessages,
      {
        prompt: "",
        profileId: config.profileId || "DEFAULT",
        parameters: config.parameters || {},
        tools: config.tools,
      },
      { abortSignal, executionId, nodeId },
    );
  }

  /**
   * Inject dynamic prompts into message array
   *
   * Two-layer injection strategy:
   * 1. staticSystem: Prepended to system message (stable content, cached)
   * 2. dynamicUserContext: Appended to last user message (variable content, not cached)
   *
   * @param messages Original message array
   * @param staticSystem System-level dynamic content
   * @param userContextSuffix User-level dynamic context
   * @returns Messages with injected dynamic prompts
   */
  private injectDynamicPrompts(
    messages: LLMMessage[],
    staticSystem: string,
    userContextSuffix: string,
  ): LLMMessage[] {
    const result: LLMMessage[] = [...messages];

    // 1. Handle system message injection
    if (staticSystem) {
      // Find existing system message
      const systemMsgIndex = result.findIndex((msg: LLMMessage) => {
        const role = msg && typeof msg === "object" && "role" in msg ? msg.role : undefined;
        return role === "system";
      });

      if (systemMsgIndex >= 0) {
        // Append to existing system message
        const msg = result[systemMsgIndex];
        if (msg && typeof msg === "object" && "content" in msg) {
          const currentContent = msg.content;
          result[systemMsgIndex] = {
            ...msg,
            content: typeof currentContent === "string"
              ? `${currentContent}\n\n${staticSystem}`
              : currentContent,
          };
        }
      } else {
        // Insert new system message at the beginning
        result.unshift({
          role: "system",
          content: staticSystem,
        } as LLMMessage);
      }
    }

    // 2. Handle user context injection (append to last user message)
    if (userContextSuffix) {
      // Find the last user message (use reverse loop for compatibility)
      let lastUserMsgIndex = -1;
      for (let i = result.length - 1; i >= 0; i--) {
        const msg = result[i];
        const role = msg && typeof msg === "object" && "role" in msg ? msg.role : undefined;
        if (role === "user") {
          lastUserMsgIndex = i;
          break;
        }
      }

      if (lastUserMsgIndex >= 0) {
        const lastUserMsg = result[lastUserMsgIndex];
        if (lastUserMsg && typeof lastUserMsg === "object" && "content" in lastUserMsg) {
          const currentContent = lastUserMsg.content;
          result[lastUserMsgIndex] = {
            ...lastUserMsg,
            content: typeof currentContent === "string"
              ? `${currentContent}\n\n${userContextSuffix}`
              : currentContent,
          };
        }
      } else {
        // No user message found, append as a new user message
        result.push({
          role: "user",
          content: userContextSuffix,
        } as LLMMessage);
      }
    }

    return result;
  }

  /**
   * Execute a single LLM call with optional tool execution
   *
   * This method performs ONE complete LLM interaction:
   * 1. Add user message to conversation
   * 2. Execute single LLM call (with transformContext applied)
   * 3. Execute tool calls if present (when executeTools=true)
   * 4. Update token usage and trigger warnings
   * 5. Trigger events (message, token, conversation state)
   *
   * Note: This is NOT a loop. If callers need multiple iterations
   * (LLM -> Tool -> LLM), they should call executeLLM() repeatedly.
   *
   * @param params Execution parameters
   * @param conversationState Conversation manager
   * @returns LLM response content or interruption state
   */
  private async executeSingleLLMCall(
    params: LLMExecutionParams,
    conversationState: ConversationSession,
  ): Promise<string | ExecutionInterruptionCheckResult> {
    const {
      contextId,
      prompt,
      config,
      tools,
      abortSignal,
      eventManager,
      nodeId,
      executeTools = true,
      transformContext,
    } = params;

    const {
      profileId,
      parameters,
      maxToolCallsPerRequest,
      enableTokenTracking,
      tokenWarningThreshold,
      tokenLimit,
    } = config;

    // Use unified interruption handler for the entire LLM execution flow
    const result = await executeWithInterruptionHandling(async signal => {
      // Step 1: Add user message
      const userMessage = {
        role: "user" as MessageRole,
        content: prompt,
      };
      conversationState.addMessage(userMessage);

      // Trigger message added event
      if (eventManager) {
        await this.triggerMessageAddedEvent(eventManager, contextId, userMessage, nodeId);
      }

      // Prepare tool schemas
      let availableToolSchemas = tools;
      if (tools && tools.length > 0) {
        availableToolSchemas = prepareToolSchemasFromTools(
          tools as Array<{ id: string; description: string; parameters: unknown }>,
        );
      }

      // Step 2: Retrieve messages and apply transformContext
      let llmMessages = conversationState.getMessages();

      // Process dynamic prompt injection if provided
      let systemPromptSuffix = "";
      let userContextSuffix = "";

      if (transformContext) {
        const dynamicContext: DynamicPromptContext = {
          timestamp: Date.now(),
          messageCount: llmMessages.length,
          executionId: contextId,
          signal,
        };

        const injection: DynamicPromptInjection = await transformContext(dynamicContext);

        if (injection.staticSystem) {
          systemPromptSuffix = injection.staticSystem;
        }
        if (injection.dynamicUserContext) {
          userContextSuffix = injection.dynamicUserContext;
        }
      }

      // Apply dynamic prompts to messages
      if (systemPromptSuffix || userContextSuffix) {
        llmMessages = this.injectDynamicPrompts(llmMessages, systemPromptSuffix, userContextSuffix);
      }

      // Execute LLM call with signal
      const llmResult = await this.llmExecutor.executeLLMCall(
        llmMessages,
        {
          prompt,
          profileId: profileId || "DEFAULT",
          parameters: parameters || {},
          tools: availableToolSchemas as ToolSchema[],
        },
        { abortSignal: signal, executionId: contextId, nodeId },
      );

      // LLM Executor now throws errors directly (including AbortError)
      // The executeWithInterruptionHandling wrapper will catch and handle interruptions
      const llmResponse = llmResult;

      // Update Token usage statistics
      if (llmResponse.usage) {
        conversationState.updateTokenUsage(llmResponse.usage as LLMUsage);

        // Record token metrics
        if (this.tokenMetricsCollector && llmResponse.usage) {
          const usage = llmResponse.usage as LLMUsage;
          this.tokenMetricsCollector.recordTokenUsage({
            profileId: params.config.profileId || "DEFAULT",
            executionId: params.contextId,
            nodeId: params.nodeId,
            totalTokens: usage.totalTokens || 0,
            promptTokens: usage.promptTokens || 0,
            completionTokens: usage.completionTokens || 0,
            cost: usage.totalCost,
          });
        }
      }

      // Finalize current request Token statistics
      conversationState.finalizeCurrentRequest();

      // Check Token usage warning AFTER updating with new usage
      if (enableTokenTracking !== false && eventManager) {
        const tokenUsage = conversationState.getTokenUsage();
        if (tokenUsage) {
          const limit = tokenLimit || 100000;
          const threshold = tokenWarningThreshold || 80;
          const usagePercentage = (tokenUsage.totalTokens / limit) * 100;

          if (usagePercentage > threshold) {
            await this.triggerTokenWarningEvent(
              eventManager,
              contextId,
              tokenUsage.totalTokens,
              limit,
              usagePercentage,
            );
          }
        }
      }

      // Add LLM response to conversation history
      const assistantMessage = {
        role: "assistant" as MessageRole,
        content: llmResponse.content,
        toolCalls: llmResponse.toolCalls?.map(
          (tc: { id: string; name: string; arguments: string }) => ({
            id: tc.id,
            type: "function" as const,
            function: {
              name: tc.name,
              arguments: tc.arguments,
            },
          }),
        ),
      };
      conversationState.addMessage(assistantMessage);

      // Trigger message added event
      if (eventManager) {
        await this.triggerMessageAddedEvent(eventManager, contextId, assistantMessage, nodeId);
      }

      // Check if there are tool calls and should execute them
      if (llmResponse.toolCalls && llmResponse.toolCalls.length > 0 && executeTools) {
        // Validate single response tool call count
        const maxToolsPerResponse = maxToolCallsPerRequest ?? 3;
        if (llmResponse.toolCalls.length > maxToolsPerResponse) {
          throw new ExecutionError(
            `LLM returned ${llmResponse.toolCalls.length} tool calls, ` +
              `exceeds limit of ${maxToolsPerResponse}. ` +
              `Configure maxToolCallsPerRequest to adjust this limit.`,
            nodeId,
          );
        }

        // Execute tool calls with signal
        await this.toolCallExecutor.executeToolCalls(
          llmResponse.toolCalls,
          conversationState,
          contextId,
          nodeId || "",
          { abortSignal: signal },
        );
      }

      // Trigger conversation state changed event
      if (eventManager) {
        const finalTokenUsage = conversationState.getTokenUsage();
        await this.triggerConversationStateChangedEvent(
          eventManager,
          contextId,
          conversationState.getMessages().length,
          finalTokenUsage?.totalTokens || 0,
          nodeId,
        );
      }

      // Return final content
      return llmResponse.content;
    }, abortSignal);

    // Handle result
    if (!result.success) {
      return result.interruption;
    }

    return result.result;
  }

  /**
   * Trigger message added event
   */
  private async triggerMessageAddedEvent(
    eventManager: EventRegistry,
    contextId: string,
    message: { role: string; content: string },
    nodeId?: string,
  ): Promise<void> {
    try {
      const event = buildMessageAddedEvent({
        executionId: contextId,
        role: message.role,
        content: message.content,
        nodeId,
      });
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to trigger message added event", { contextId, error });
    }
  }

  /**
   * Trigger token usage warning event
   */
  private async triggerTokenWarningEvent(
    eventManager: EventRegistry,
    contextId: string,
    tokensUsed: number,
    tokenLimit: number,
    usagePercentage: number,
  ): Promise<void> {
    try {
      const event = buildTokenUsageWarningEvent({
        executionId: contextId,
        tokensUsed,
        tokenLimit,
        usagePercentage,
      });
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to trigger token warning event", { contextId, error });
    }
  }

  /**
   * Trigger conversation state changed event
   */
  private async triggerConversationStateChangedEvent(
    eventManager: EventRegistry,
    contextId: string,
    messageCount: number,
    tokenUsage: number,
    nodeId?: string,
  ): Promise<void> {
    try {
      const event = buildConversationStateChangedEvent({
        executionId: contextId,
        messageCount,
        tokenUsage,
        nodeId,
      });
      await eventManager.emit(event);
    } catch (error) {
      logger.warn("Failed to trigger conversation state changed event", { contextId, error });
    }
  }
}
