/**
 * Agent Loop Node Processor
 *
 * Responsible for processing AgentLoop nodes within the Graph execution engine.
 * Uses the new AgentLoopCoordinator architecture, which supports pause/resume functionality.
 * Supports referencing named message contexts via initialContextRefs.
 */

import type {
  RuntimeNode,
  WorkflowExecution,
  AgentLoopNodeConfig,
  LLMMessage,
  MessageContextRegistry,
} from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import { now, diffTimestamp, getErrorOrNew } from "@wf-agent/common-utils";

import type { ConversationSession } from "../../../../shared/messaging/conversation-session.js";
import type { EventRegistry } from "../../../../shared/registry/event-registry.js";
import { AgentLoopCoordinator, AgentLoopExecutor } from "../../../../agent/index.js";
import type { AgentLoopRegistry } from "../../../../agent/index.js";
import { emit } from "../../../../shared/events/emit-event.js";
import {
  buildMessageAddedEvent,
  buildConversationStateChangedEvent,
} from "../../../../shared/events/builders/index.js";
import { LLMExecutor } from "../../../../services/executors/llm-executor.js";
import { ToolRegistry } from "../../../../shared/registry/tool-registry.js";
import type { SkillRegistry } from "../../../../shared/registry/skill-registry.js";
import * as Identifiers from "../../../../di/service-identifiers.js";
import type { GlobalContext } from "../../../../shared/global-context.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { injectSkillMetadata } from "../../../../shared/utils/metadata-injection.js";

const logger = createContextualLogger({ component: "AgentLoopHandler" });

/**
 * Agent Loop node execution results
 */
export interface AgentLoopExecutionResult {
  /** Execution Status */
  status: "COMPLETED" | "FAILED" | "ABORTED" | "PAUSED";
  /** The final LLM response content (alias for AgentLoopNodeOutput.finalResponse) */
  content?: string;
  /** The final LLM response content (public output field) */
  finalResponse?: string;
  /** Actual number of iterations (alias for AgentLoopNodeOutput.iterationCount) */
  iterations?: number;
  /** Number of iterations (public output field) */
  iterationCount?: number;
  /** Number of tool calls */
  toolCallCount?: number;
  /** Did it end because the maximum number of iterations was reached? */
  hitIterationLimit?: boolean;
  /** Error message (in case of failure) */
  error?: Error;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Agent Loop instance ID (used for pausing/resuming) */
  loopId?: string;
  /**
   * Inner error records from the agent loop execution state.
   * Provides cross-layer error traceability: when this agent loop fails
   * inside a workflow, the workflow's root cause analysis can access
   * these records to trace into the agent's internal error chain.
   */
  innerErrorRecords?: Array<{
    id: string;
    timestamp: number;
    message: string;
    errorType: string;
    severity: string;
    iteration?: number;
    context: { operation: string; toolName?: string };
  }>;
}

/**
 * Agent Loop Processor Context
 */
export interface AgentLoopHandlerContext {
  /** LLM Executor */
  llmExecutor: LLMExecutor;
  /** Tool Services */
  toolService: ToolRegistry;
  /** Dialogue Manager */
  conversationManager: ConversationSession;
  /** Event Manager */
  eventManager: EventRegistry;
  /** Agent Loop registry (optional, used for cross-request management) */
  agentLoopRegistry?: AgentLoopRegistry;
  /** Execution Registry (used for checking interrupts) */
  executionRegistry?: unknown;
  /** WorkflowExecutionEntity reference (for VariableManager access) */
  workflowExecutionEntity?: {
    variableStateManager: {
      setVariable: (name: string, value: unknown) => void;
      getVariable: (name: string) => unknown;
    };
    getInput(): Record<string, unknown>;
  };
}

/**
 * Collect messages from initial context references
 *
 * Reads from named message contexts via messageInputs configuration.
 * Each entry specifies a source context name and internal mapping name.
 * Messages from all specified contexts are concatenated into the initial
 * message list for the agent loop.
 *
 * If messageInputs is not defined, returns an empty array.
 * The agent loop will start with no initial messages.
 */
function collectInitialMessages(
  config: AgentLoopNodeConfig,
  workflowExecution: WorkflowExecution,
): LLMMessage[] {
  const messageInputs = config.inlineConfig?.messageInputs;
  if (!messageInputs || messageInputs.length === 0) {
    return [];
  }

  const registry = (
    workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }
  ).messageContextRegistry;
  if (!registry) {
    throw new RuntimeValidationError("MessageContextRegistry not found in execution context", {
      operation: "collectInitialMessages",
      field: "messageContextRegistry",
    });
  }

  const allMessages: LLMMessage[] = [];
  for (const inputDef of messageInputs) {
    const { sourceContextId, internalName, required, defaultMessages } = inputDef;
    const namedContext = registry.get(sourceContextId);

    if (namedContext) {
      allMessages.push(...namedContext.messages);
    } else if (required) {
      throw new RuntimeValidationError(
        `Required message context '${sourceContextId}' (mapped to '${internalName}') not found in registry`,
        { operation: "collectInitialMessages", field: "messageInputs", value: sourceContextId },
      );
    } else if (defaultMessages && defaultMessages.length > 0) {
      allMessages.push(...defaultMessages);
    }
  }
  return allMessages;
}

/**
 * Sync completion data/variables from attempt_completion to workflow VariableManager
 *
 * The attempt_completion tool emits an ATTEMPT_COMPLETION event and its
 * data/variables are returned via AgentLoopResult.completionData.
 * This function applies those changes to the workflow VariableManager.
 */
function syncCompletionData(
  completionData: { data?: Record<string, unknown>; variables?: Record<string, unknown> } | undefined,
  variableStateManager?: { setVariable: (name: string, value: unknown) => void; getVariable: (name: string) => unknown },
): void {
  if (!completionData || !variableStateManager) return;

  if (completionData.data) {
    for (const [key, value] of Object.entries(completionData.data)) {
      const existing = (variableStateManager.getVariable(key) as unknown[]) || [];
      if (Array.isArray(value)) {
        variableStateManager.setVariable(key, [...existing, ...value]);
      } else {
        variableStateManager.setVariable(key, [...existing, value]);
      }
    }
  }

  if (completionData.variables) {
    for (const [key, value] of Object.entries(completionData.variables)) {
      variableStateManager.setVariable(key, value);
    }
  }

  logger.debug("Synced completion data to workflow variables", {
    dataKeys: completionData.data ? Object.keys(completionData.data) : [],
    variableKeys: completionData.variables ? Object.keys(completionData.variables) : [],
  });
}

/**
 * Sync agent loop messages back to the workflow MessageContextRegistry
 *
 * After the agent loop completes, this function maps the agent's accumulated
 * conversation messages to named contexts in the workflow registry, as defined
 * by the messageOutputs configuration.
 */
function syncMessageOutputs(
  config: AgentLoopNodeConfig,
  workflowExecution: WorkflowExecution,
  conversationManager: ConversationSession,
): void {
  const messageOutputs = config.inlineConfig?.messageOutputs;
  if (!messageOutputs || messageOutputs.length === 0) {
    return;
  }

  const registry = (
    workflowExecution as WorkflowExecution & { messageContextRegistry?: MessageContextRegistry }
  ).messageContextRegistry;
  if (!registry) {
    logger.warn("MessageContextRegistry not available for syncing message outputs");
    return;
  }

  const allMessages = conversationManager.getAllMessages();
  const now_ts = Date.now();

  for (const outputDef of messageOutputs) {
    const { internalName, targetContextId } = outputDef;
    registry.register({
      id: targetContextId,
      messages: [...allMessages],
      createdAt: now_ts,
      updatedAt: now_ts,
      metadata: {
        source: "agent-loop",
        internalName,
        messageCount: allMessages.length,
      } as Record<string, unknown>,
    });

    logger.debug("Synced agent loop messages to context", {
      contextId: targetContextId,
      messageCount: allMessages.length,
    });
  }
}

/**
 * Create an AgentLoopCoordinator instance
 */
function createCoordinator(
  globalContext: GlobalContext,
  context: AgentLoopHandlerContext,
): AgentLoopCoordinator {
  // Get AgentLoopRegistry from DI container
  const registry =
    context.agentLoopRegistry ??
    (globalContext.container.get(Identifiers.AgentLoopRegistry) as AgentLoopRegistry);
  const executor = new AgentLoopExecutor({
    llmExecutor: context.llmExecutor,
    toolService: context.toolService,
    eventManager: context.eventManager,
    globalContext,
  });

  return new AgentLoopCoordinator(registry, executor, globalContext, context.eventManager);
}

/**
 * Resolved runtime configuration for agent loop execution
 *
 * This is the merged result of agentLoopId (static definition) and
 * inlineConfig (selective overrides). At runtime, the handler only
 * uses this resolved config — the definition-to-override merge is
 * expected to happen at workflow definition load/parse time.
 */
interface ResolvedAgentRuntimeConfig {
  profileId: string;
  maxIterations?: number;
  systemPrompt?: string;
  availableTools?: import("@wf-agent/types").AgentToolConfig;
  dataInputs?: import("@wf-agent/types").WorkflowDataInput[];
  messageInputs?: import("@wf-agent/types").WorkflowMessageInput[];
  messageOutputs?: import("@wf-agent/types").WorkflowMessageOutput[];
  workingContext?: string;
}

/**
 * Resolve the final agent loop runtime config from AgentLoopNodeConfig.
 *
 * Priority order:
 * 1. If inlineConfig is present, use its fields as the primary source.
 * 2. If agentLoopId is provided without inlineConfig, the config resolver
 *    should have pre-merged the static definition at load time.
 *    For now, this requires inlineConfig to be available.
 *
 * The agentLoopId field is preserved on the node config for reference/tracking
 * (e.g., logging, monitoring), not for runtime config resolution.
 */
function resolveAgentRuntimeConfig(config: AgentLoopNodeConfig): ResolvedAgentRuntimeConfig {
  const inlineConfig = config.inlineConfig || {};

  const resolved: ResolvedAgentRuntimeConfig = {
    profileId: inlineConfig.profileId || "",
    maxIterations: inlineConfig.maxIterations,
    systemPrompt: inlineConfig.systemPrompt || "",
    availableTools: inlineConfig.availableTools,
    dataInputs: inlineConfig.dataInputs,
    messageInputs: inlineConfig.messageInputs,
    messageOutputs: inlineConfig.messageOutputs,
    workingContext: inlineConfig.workingContext,
  };

  if (resolved.profileId) {
    return resolved;
  }

  // If no profileId in inlineConfig, agentLoopId must be provided for resolution.
  // The static definition lookup (agentLoopId → AgentLoopDefinition) is expected
  // to happen at workflow definition load time, producing the merged inlineConfig.
  // At runtime, the handler receives the pre-merged config.
  throw new RuntimeValidationError(
    "AgentLoop node requires a profileId. Provide it via inlineConfig.profileId or ensure agentLoopId is resolved at workflow load time.",
    {
      operation: "resolveAgentRuntimeConfig",
      field: "profileId",
      value: config.agentLoopId,
      context: {
        agentLoopId: config.agentLoopId,
        hasInlineConfig: !!config.inlineConfig,
      },
    },
  );
}

interface AgentLoopExecutionEntity {
  getWorkflowExecutionData(): WorkflowExecution;
  getInput?(): Record<string, unknown>;
}

/**
 * Agent Loop Node Processor
 */
export async function agentLoopHandler(
  globalContext: GlobalContext,
  executionEntity: AgentLoopExecutionEntity,
  node: RuntimeNode,
  context: AgentLoopHandlerContext,
): Promise<AgentLoopExecutionResult> {
  const execution = executionEntity.getWorkflowExecutionData();
  const config = node.config as AgentLoopNodeConfig;
  const startTime = now();

  try {
    // Resolve runtime config: merge agentLoopId + inlineConfig
    const resolvedConfig = resolveAgentRuntimeConfig(config);

    // 1. Prepare the initial messages from context references
    const initialMessages = collectInitialMessages(config, execution);

    // Process dataInputs: map execution input data fields to internal variables
    const inlineConfig = config.inlineConfig;
    if (inlineConfig?.dataInputs && inlineConfig.dataInputs.length > 0) {
      const varManager = context.workflowExecutionEntity?.variableStateManager;
      const input = executionEntity.getInput ? executionEntity.getInput() : execution.input || {};
      for (const inputDef of inlineConfig.dataInputs) {
        const { parentField, internalName, required, defaultValue } = inputDef;
        let value = input[parentField];
        if (value === undefined) {
          if (defaultValue !== undefined) {
            value = defaultValue;
          } else if (required) {
            throw new RuntimeValidationError(
              `Required data input '${parentField}' (mapped to variable '${internalName}') is missing`,
              { operation: "agentLoopHandler", field: parentField },
            );
          }
        }
        if (value !== undefined && varManager) {
          varManager.setVariable(internalName, value);
        }
      }
    }

    // Add input prompt if available
    const inputPromptManager = context.workflowExecutionEntity?.variableStateManager;
    const inputPrompt =
      inputPromptManager?.getVariable("input") || inputPromptManager?.getVariable("prompt");

    if (inputPrompt && typeof inputPrompt === "string") {
      initialMessages.push({ role: "user", content: inputPrompt });

      // Trigger message addition event
      try {
        await emit(
          context.eventManager,
          buildMessageAddedEvent({
            executionId: execution.id,
            role: "user",
            content: inputPrompt,
            nodeId: node.id,
          }),
        );
      } catch (error) {
        logger.debug("Failed to emit MESSAGE_ADDED event", { error });
      }
    }

    // 2. Inject skill metadata into system prompt if skills are configured
    // Only inject if 'skill' tool is in availableTools (or auto-add if skills exist)
    try {
      const skillRegistry = globalContext.container.get(Identifiers.SkillRegistry) as
        | SkillRegistry
        | undefined;
      const skillResult = injectSkillMetadata(skillRegistry, {
        systemPrompt: resolvedConfig.systemPrompt || "",
        availableTools: resolvedConfig.availableTools,
        autoAddTool: true,
      });

      resolvedConfig.systemPrompt = skillResult.systemPrompt;
      // Type assertion: injectSkillMetadata preserves AgentToolConfig type when input is AgentToolConfig
      resolvedConfig.availableTools =
        skillResult.availableTools as typeof resolvedConfig.availableTools;

      if (skillResult.injected) {
        logger.debug("Skill metadata injected", {
          skillCount: skillResult.skillCount,
          nodeId: node.id,
        });
      }
    } catch (error) {
      logger.debug("Skills not configured, skipping skill metadata injection", { error });
    }

    // 3. Create a Coordinator and execute it.
    const coordinator = createCoordinator(globalContext, context);

    const result = await coordinator.execute(
      {
        profileId: resolvedConfig.profileId,
        systemPrompt: resolvedConfig.systemPrompt,
        initialMessages,
        availableTools: resolvedConfig.availableTools,
        maxIterations: resolvedConfig.maxIterations,
      },
      {
        conversationManager: context.conversationManager,
        parentExecutionId: execution.id,
        nodeId: node.id,
      },
    );

    if (!result.success) {
      // Attach inner error records to the thrown error for cross-layer traceability.
      // The catch block below will forward them to the AgentLoopExecutionResult.
      const thrownError = result.error || new Error("Agent loop failed");
      if (result.innerErrorRecords && result.innerErrorRecords.length > 0) {
        (thrownError as any).innerErrorRecords = result.innerErrorRecords;
      }
      throw thrownError;
    }

    // 3. Synchronize messages to ConversationSession
    // In the new architecture, messages have been automatically synchronized to the ConversationSession through the AgentLoopEntity
    // Here only the event needs to be triggered
    if (result.content) {
      try {
        await emit(
          context.eventManager,
          buildMessageAddedEvent({
            executionId: execution.id,
            role: "assistant",
            content: result.content,
            nodeId: node.id,
          }),
        );
      } catch (error) {
        logger.debug("Failed to emit MESSAGE_ADDED event", { error });
      }
    }

    // Trigger a dialog state change event
    try {
      await emit(
        context.eventManager,
        buildConversationStateChangedEvent({
          executionId: execution.id,
          messageCount: context.conversationManager.getMessages().length,
          tokenUsage: 0, // Not counting the total consumption for now.
          nodeId: node.id,
        }),
      );
    } catch (error) {
      logger.debug("Failed to emit CONVERSATION_STATE_CHANGED event", { error });
    }

    // Sync agent loop messages back to workflow MessageContextRegistry
    syncMessageOutputs(config, execution, context.conversationManager);

    // Sync completion data/variables from attempt_completion result to workflow VariableManager
    syncCompletionData(result.completionData, context.workflowExecutionEntity?.variableStateManager);

    // 4. Update the variable using VariableManager API
    const updateVarManager = context.workflowExecutionEntity?.variableStateManager;
    if (updateVarManager) {
      updateVarManager.setVariable("finalResponse", result.content);
      updateVarManager.setVariable("iterationCount", result.iterations);
      updateVarManager.setVariable("toolCallCount", result.toolCallCount);
    }

    return {
      status: "COMPLETED",
      finalResponse: result.content,
      content: result.content,
      iterationCount: result.iterations,
      iterations: result.iterations,
      toolCallCount: result.toolCallCount,
      executionTime: diffTimestamp(startTime, now()),
    };
  } catch (error) {
    // Extract inner error records attached by the throw path above
    const innerErrorRecords = (error as any)?.innerErrorRecords as
      | AgentLoopExecutionResult["innerErrorRecords"]
      | undefined;

    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, now()),
      // Cross-layer error traceability: forward inner agent error records
      innerErrorRecords,
    };
  }
}

/**
 * Stream Agent Loop Node Processor
 */
export async function* agentLoopStreamHandler(
  globalContext: GlobalContext,
  execution: WorkflowExecution,
  node: RuntimeNode,
  context: AgentLoopHandlerContext,
): AsyncGenerator<unknown, AgentLoopExecutionResult, unknown> {
  const config = node.config as AgentLoopNodeConfig;
  const startTime = now();

  try {
    // Resolve runtime config: merge agentLoopId + inlineConfig
    const resolvedConfig = resolveAgentRuntimeConfig(config);

    // 1. Prepare the initial messages from context references
    const initialMessages = collectInitialMessages(config, execution);

    // Process dataInputs: map execution input data fields to internal variables
    const inlineConfig = config.inlineConfig;
    if (inlineConfig?.dataInputs && inlineConfig.dataInputs.length > 0) {
      const varManager = context.workflowExecutionEntity?.variableStateManager;
      const input = context.workflowExecutionEntity?.getInput
        ? context.workflowExecutionEntity.getInput()
        : execution.input || {};
      for (const inputDef of inlineConfig.dataInputs) {
        const { parentField, internalName, required, defaultValue } = inputDef;
        let value = input[parentField];
        if (value === undefined) {
          if (defaultValue !== undefined) {
            value = defaultValue;
          } else if (required) {
            throw new RuntimeValidationError(
              `Required data input '${parentField}' (mapped to variable '${internalName}') is missing`,
              { operation: "agentLoopStreamHandler", field: parentField },
            );
          }
        }
        if (value !== undefined && varManager) {
          varManager.setVariable(internalName, value);
        }
      }
    }

    // Add input prompt if available
    const inputPromptManager = context.workflowExecutionEntity?.variableStateManager;
    const inputPrompt =
      inputPromptManager?.getVariable("input") || inputPromptManager?.getVariable("prompt");

    if (inputPrompt && typeof inputPrompt === "string") {
      initialMessages.push({ role: "user", content: inputPrompt });
    }

    // 2. Create a Coordinator to execute tasks in parallel.
    const coordinator = createCoordinator(globalContext, context);

    for await (const event of coordinator.executeStream(
      {
        profileId: resolvedConfig.profileId,
        systemPrompt: resolvedConfig.systemPrompt,
        initialMessages,
        availableTools: resolvedConfig.availableTools,
        maxIterations: resolvedConfig.maxIterations,
      },
      {
        conversationManager: context.conversationManager,
        parentExecutionId: execution.id,
        nodeId: node.id,
      },
    )) {
      // Forward streaming events
      yield {
        type: "agent_loop_event",
        executionId: execution.id,
        nodeId: node.id,
        event,
      };
    }

    // 3. Obtain the execution results
    const entity = coordinator.getRunning()[0] || coordinator.getPaused()[0];
    const iterations = entity?.state.currentIteration ?? 0;
    const toolCallCount = entity?.state.toolCallCount ?? 0;
    const stateCoordinator = entity ? coordinator.getStateCoordinator(entity.id) : null;
    const content = stateCoordinator
      ?.getMessages()
      .filter((m: { role: string; content: unknown }) => m.role === "assistant")
      .pop()?.content;

    // 4. Update the variable using VariableManager API
    const updateManager = context.workflowExecutionEntity?.variableStateManager;
    if (updateManager) {
      updateManager.setVariable("finalResponse", content);
      updateManager.setVariable("iterationCount", iterations);
      updateManager.setVariable("toolCallCount", toolCallCount);
    }

    // Sync agent loop messages back to workflow MessageContextRegistry
    syncMessageOutputs(config, execution, context.conversationManager);

    // Sync completion data/variables from attempt_completion result to workflow VariableManager
    syncCompletionData(undefined, context.workflowExecutionEntity?.variableStateManager);

    return {
      status: entity?.isPaused() ? "PAUSED" : "COMPLETED",
      finalResponse: typeof content === "string" ? content : undefined,
      content: typeof content === "string" ? content : undefined,
      iterationCount: iterations,
      iterations,
      toolCallCount,
      executionTime: diffTimestamp(startTime, now()),
      loopId: entity?.id,
    };
  } catch (error) {
    return {
      status: "FAILED",
      error: getErrorOrNew(error),
      executionTime: diffTimestamp(startTime, now()),
    };
  }
}
